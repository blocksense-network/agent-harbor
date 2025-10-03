# frozen_string_literal: true

# Command Line Interface Specification
# ====================================
#
# This module implements the core functionality for all agent-harbor commands:
#
# `agent-task` (CLI.start_task) is used to record coding tasks that will be
# developed by AI agents. It has two modes of operation:
#
# 1) Starting a new agent branch (you pass the branch name as argument to agent-task)
# 2) Recording a new task in the current branch (it needs to be a agent branch already)

module AgentTask
  # CLI exposes the main binaries as callable methods so the functionality
  # can be reused programmatically. The methods here mirror the behavior
  # of the command line tools.
  module CLI # rubocop:disable Metrics/ModuleLength
    module_function

    def devshell_names(root)
      file = File.join(root, 'flake.nix')
      return [] unless File.exist?(file)

      # First try using nix eval to properly parse the flake
      # This handles all possible Nix syntax variations correctly
      begin
        # Get the current system first
        system_result = `nix eval --impure --raw --expr 'builtins.currentSystem' 2>#{File::NULL}`.strip
        require 'English'
        if $CHILD_STATUS.success?
          current_system = system_result

          # Evaluate the devShells attribute for the current system
          eval_cmd = [
            'nix', 'eval', '--json', '--no-warn-dirty',
            "#{file}#devShells.#{current_system}",
            '--apply', 'builtins.attrNames'
          ]

          output_io = IO.popen(eval_cmd, 'r', err: File::NULL)
          result = output_io.read
          status = output_io.close
          if status
            require 'json'
            return JSON.parse(result)
          end
        end
      rescue StandardError
        # Continue to fallback
      end

      # Fallback: try to get all devShells regardless of system
      begin
        nix_expr = 'devShells: let systems = builtins.attrNames devShells; ' \
                   'in if systems == [] then [] ' \
                   'else builtins.attrNames (devShells.${builtins.head systems})'
        eval_cmd = [
          'nix', 'eval', '--json', '--no-warn-dirty',
          "#{file}#devShells",
          '--apply', nix_expr
        ]

        output_io = IO.popen(eval_cmd, 'r', err: File::NULL)
        result = output_io.read
        status = output_io.close
        if status
          require 'json'
          return JSON.parse(result)
        end
      rescue StandardError
        # Continue to final fallback
      end

      # Final fallback to regex parsing for malformed flakes (e.g., in tests)
      content = File.read(file)
      content.scan(/devShells\.[^.]+\.([A-Za-z0-9._-]+)\s*=/).map { |match| match[0] }.uniq
    end

    def discover_repos
      require_relative '../vcs_repo'
      require_relative '../agent_tasks'

      repos = []
      begin
        repos << [nil, AgentTasks.new]
        return repos
      rescue RepositoryNotFoundError
        # continue to scan subdirectories
      end

      Dir.children(Dir.pwd).sort.each do |entry|
        candidate = File.join(Dir.pwd, entry)
        next unless File.directory?(candidate)

        vcs_dirs = %w[.git .hg .bzr .fslckout _FOSSIL_]
        next unless vcs_dirs.any? { |v| File.exist?(File.join(candidate, v)) || Dir.exist?(File.join(candidate, v)) }

        begin
          repos << [entry, AgentTasks.new(candidate)]
        rescue StandardError
          next
        end
      end

      repos
    end

    EDITOR_HINT = <<~HINT
      # Please write your task prompt above.
      # Enter an empty prompt to abort the task creation process.
      # Feel free to leave this comment in the file. It will be ignored.
    HINT

    # Implements the same workflow as the `agent-task` executable.
    # Arguments should match the command line invocation.
    def start_task(args, stdin: $stdin, stdout: $stdout)
      require 'tempfile'
      require 'fileutils'
      require 'time'
      require 'optparse'
      require_relative '../vcs_repo'
      require_relative '../agent_tasks'

      options = {}
      OptionParser.new do |opts|
        opts.on('--push-to-remote=BOOL', 'Push branch to remote automatically') do |val|
          options[:push_to_remote] = val
        end
        opts.on('--prompt=STRING', 'Use STRING as the task prompt') do |val|
          options[:prompt] = val
        end
        opts.on('--prompt-file=FILE', 'Read the task prompt from FILE') do |val|
          options[:prompt_file] = val
        end
        opts.on('-sNAME', '--devshell=NAME', 'Record the dev shell name in the commit') do |val|
          options[:devshell] = val
        end
      end.parse!(args)

      branch_name = args.shift
      start_new_branch = branch_name && !branch_name.strip.empty?
      abort('Error: --prompt and --prompt-file are mutually exclusive') if options[:prompt] && options[:prompt_file]

      prompt_content = nil
      if options[:prompt]
        prompt_content = options[:prompt].dup
      elsif options[:prompt_file]
        begin
          prompt_content = File.read(options[:prompt_file])
        rescue StandardError => e
          abort("Error: Failed to read prompt file: #{e.message}")
        end
      end

      begin
        repo = VCSRepo.new
      rescue StandardError => e
        stdout.puts e.message
        exit 1
      end

      orig_branch = repo.current_branch
      if start_new_branch
        begin
          repo.start_branch(branch_name)
        rescue StandardError => e
          stdout.puts e.message
          exit 1
        end
        if options[:devshell]
          flake_path = File.join(repo.root, 'flake.nix')
          abort('Error: Repository does not contain a flake.nix file') unless File.exist?(flake_path)
          shells = devshell_names(repo.root)
          unless shells.include?(options[:devshell])
            abort("Error: Dev shell '#{options[:devshell]}' not found in flake.nix")
          end
        end
      else
        branch_name = orig_branch
        main_names = [repo.default_branch, 'main', 'master', 'trunk', 'default']
        abort('Error: Refusing to run on the main branch') if main_names.include?(branch_name)
        abort('Error: --devshell is only supported when creating a new branch') if options[:devshell]
      end

      cleanup_branch = start_new_branch

      begin
        task_content = nil
        if prompt_content.nil?
          tempfile = Tempfile.new(['task', '.txt'])
          tempfile.write("\n")
          tempfile.write(EDITOR_HINT)
          tempfile.close

          editor = ENV.fetch('EDITOR', nil)
          unless editor
            editors = %w[nano pico micro vim helix vi]
            editors.each do |ed|
              if system('command', '-v', ed, out: File::NULL, err: File::NULL)
                editor = ed
                break
              end
            end
            editor ||= 'nano'
          end

          abort('Error: Failed to open the editor.') unless system("#{editor} #{tempfile.path}")
          task_content = File.read(tempfile.path)
          task_content.sub!("\n#{EDITOR_HINT}", '')
          task_content.sub!(EDITOR_HINT, '')
        else
          task_content = prompt_content
        end
        task_content.gsub!("\r\n", "\n")
        abort('Aborted: empty task prompt.') if task_content.strip.empty?

        tasks = AgentTasks.new(repo.root)
        if start_new_branch
          tasks.record_initial_task(task_content, branch_name, devshell: options[:devshell])
        else
          tasks.append_task(task_content)
        end

        push = nil
        if options.key?(:push_to_remote)
          val = options[:push_to_remote].to_s.downcase
          truthy = %w[1 true yes y].include?(val)
          falsy = %w[0 false no n].include?(val)
          abort("Error: Invalid value for --push-to-remote: '#{options[:push_to_remote]}'") unless truthy || falsy
          push = truthy
        else
          stdout.print 'Push to default remote? [Y/n]: '
          input = stdin.gets
          abort('Error: Non-interactive environment, use --push-to-remote option.') if input.nil?
          answer = input.strip
          answer = 'y' if answer.empty?
          push = answer.downcase.start_with?('y')
        end
        repo.push_current_branch(branch_name) if push

        cleanup_branch = false
      ensure
        repo.checkout_branch(orig_branch) if orig_branch
        if cleanup_branch
          case repo.vcs_type
          when :git
            system('git', 'branch', '-D', branch_name, chdir: repo.root, out: File::NULL, err: File::NULL)
          when :fossil
            system('fossil', 'branch', 'close', branch_name, chdir: repo.root, out: File::NULL, err: File::NULL)
          end
        end
      end
    end

    # Print the current task description, replicating the `get-task` command.
    def run_get_task(args = [])
      require 'resolv'
      require 'fileutils'
      require 'optparse'
      require_relative '../vcs_repo'
      require_relative '../agent_tasks'

      options = {}
      OptionParser.new do |opts|
        opts.on('--autopush', 'Tells the agent to automatically push its changes') do
          options[:autopush] = true
        end
        opts.on('--get-setup-env', 'Print ENV vars from @agents-setup directives') do
          options[:get_setup_env] = true
        end
      end.parse!(args)

      repos = discover_repos
      if repos.empty?
        puts "Error: Could not find repository root from #{Dir.pwd}"
        exit 1
      end

      if repos.length == 1 && repos[0][0].nil?
        at = repos[0][1]
        if options[:get_setup_env]
          _, env = at.agent_prompt_with_env
          env.each { |k, v| puts "#{k}=#{v}" }
        else
          puts at.agent_prompt_with_autopush_setup(autopush: options[:autopush])
        end
        return
      end

      dir_messages = []
      repos.each do |dir, agent_tasks|
        next if dir.nil?

        begin
          if options[:get_setup_env]
            _, env = agent_tasks.agent_prompt_with_env
            dir_messages << [dir, env.map { |k, v| "#{k}=#{v}" }.join("\n")]
          else
            msg = agent_tasks.agent_prompt_with_autopush_setup(autopush: options[:autopush])
            dir_messages << [dir, msg] if msg && !msg.empty?
          end
        rescue StandardError
          next
        end
      end

      if dir_messages.empty?
        puts "Error: Could not find repository root from #{Dir.pwd}"
        exit 1
      elsif dir_messages.length == 1
        puts dir_messages[0][1]
      else
        output = dir_messages.map { |dir, msg| "In directory `#{dir}`:\n#{msg}" }.join("\n\n")
        puts output
      end
    rescue StandardError => e
      puts e.message
      exit 1
    end

    def run_start_work(args = [])
      require 'optparse'
      require_relative '../agent_tasks'

      options = {}
      OptionParser.new do |opts|
        opts.on('--task-description=DESC', 'Record the given task description') do |val|
          options[:task_description] = val
        end
        opts.on('--branch-name=NAME', 'Name to use for the task description file') do |val|
          options[:branch_name] = val
        end
      end.parse!(args)

      repos = discover_repos
      if repos.empty?
        puts "Error: Could not find repository root from #{Dir.pwd}"
        exit 1
      end

      repos.to_h.each_value do |at|
        if options[:task_description]
          if at.on_task_branch?
            at.append_task(options[:task_description])
          else
            unless options[:branch_name]
              raise StandardError, 'Error: --branch-name is required when not on an agent branch'
            end

            at.record_initial_task(options[:task_description], options[:branch_name])
          end
        end
      end
    rescue StandardError => e
      puts e.message
      exit 1
    end

    def run_setup(_args = [])
      require 'English'

      codex_version = `codex --version 2>&1`
      codex_version = $CHILD_STATUS.success? ? codex_version.strip : 'not found'

      goose_version = `goose --version 2>&1`
      goose_version = $CHILD_STATUS.success? ? goose_version.strip : 'not found'

      puts "codex: #{codex_version}"
      puts "goose: #{goose_version}"
    rescue StandardError => e
      puts e.message
      exit 1
    end
  end
end
