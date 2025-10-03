# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative 'test_helper'
require_relative '../lib/agent_tasks'

class WorkflowTest < Minitest::Test
  include RepoTestHelper

  def test_workflow_expansion_and_env
    repo, remote = setup_repo(:git)
    wf_dir = File.join(repo, '.agents', 'workflows')
    FileUtils.mkdir_p(wf_dir)
    hello = File.join(wf_dir, 'hello')
    File.write(hello, <<~SH)
      #!/bin/sh
      echo hello
      echo '@agents-setup FOO=bar'
    SH
    FileUtils.chmod(0o755, hello)
    bye = File.join(wf_dir, 'bye')
    File.write(bye, <<~SH)
      #!/bin/sh
      echo bye
    SH
    FileUtils.chmod(0o755, bye)

    prompt = <<~PROMPT
      /hello
      This task uses two workflows.
      /bye
      @agents-setup BAZ=1
    PROMPT
    status, = run_agent_task(repo, branch: 'feat', prompt: prompt, push_to_remote: false)
    # agent-task should succeed when workflows run
    assert_equal 0, status.exitstatus

    VCSRepo.new(repo).checkout_branch('feat')
    _, output = run_get_task(repo)
    # output should include results from both workflows
    assert_includes output, 'hello'
    assert_includes output, 'bye'
    # setup directives should be stripped from the output
    refute_includes output, '@agents-setup'

    cmd = if windows?
            ['ruby', RepoTestHelper::GET_TASK, '--get-setup-env']
          else
            [RepoTestHelper::GET_TASK, '--get-setup-env']
          end

    env_output = IO.popen(cmd, chdir: repo, &:read)
    status2 = $CHILD_STATUS
    # `get-task --get-setup-env` should succeed
    assert_equal 0, status2.exitstatus
    lines = env_output.split("\n")
    # env vars from workflow and task should be present
    assert_includes lines, 'FOO=bar'
    assert_includes lines, 'BAZ=1'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end
end

class WorkflowAdditionalTest < Minitest::Test
  include RepoTestHelper

  def create_workflow(repo, name, content, mode: 0o755)
    dir = File.join(repo, '.agents', 'workflows')
    FileUtils.mkdir_p(dir)
    path = File.join(dir, name)
    File.write(path, content)
    File.chmod(mode, path)
    path
  end

  def test_ruby_workflow_command
    # TODO: This works on my Windows machine, but fails mysteriously in the Windows CI
    skip if windows?
    repo, remote = setup_repo(:git)
    create_workflow(repo, 'ruby_wf', <<~RUBY)
      #!/usr/bin/env ruby
      puts 'ruby works'
      puts '@agents-setup RUBY_FLAG=1'
    RUBY

    prompt = <<~PROMPT
      This task demonstrates a ruby workflow.
      /ruby_wf
    PROMPT
    status, = run_agent_task(repo, branch: 'feat', prompt: prompt, push_to_remote: false)
    # agent-task should succeed with ruby workflow
    assert_equal 0, status.exitstatus

    VCSRepo.new(repo).checkout_branch('feat')
    _, output = run_get_task(repo)
    # get-task should include workflow output
    assert_includes output, 'ruby works'

    cmd = if windows?
            ['ruby', RepoTestHelper::GET_TASK, '--get-setup-env']
          else
            [RepoTestHelper::GET_TASK, '--get-setup-env']
          end
    env_out = IO.popen(cmd, chdir: repo, &:read)
    status2 = $CHILD_STATUS
    # env listing should succeed
    assert_equal 0, status2.exitstatus
    # ruby workflow env vars should be present
    assert_includes env_out.split("\n"), 'RUBY_FLAG=1'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_text_workflow_command
    repo, remote = setup_repo(:git)
    dir = File.join(repo, '.agents', 'workflows')
    FileUtils.mkdir_p(dir)
    File.write(File.join(dir, 'info.txt'), 'hello from txt')

    prompt = <<~PROMPT
      /info
      Some additional details about the task.
    PROMPT
    status, = run_agent_task(repo, branch: 'feat', prompt: prompt, push_to_remote: false)
    # agent-task should succeed with txt workflow
    assert_equal 0, status.exitstatus

    VCSRepo.new(repo).checkout_branch('feat')
    _, output = run_get_task(repo)
    # get-task should include contents of the txt file
    assert_includes output, 'hello from txt'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_workflow_with_arguments
    repo, remote = setup_repo(:git)
    create_workflow(repo, 'echo_args', <<~SH)
      #!/bin/sh
      echo $1 $2
    SH

    prompt = <<~PROMPT
      Before running commands.
      /echo_args foo "bar baz"
      /echo_args qux quux
      After commands.
    PROMPT
    status, = run_agent_task(repo, branch: 'feat', prompt: prompt, push_to_remote: false)
    # agent-task should succeed when passing arguments twice
    assert_equal 0, status.exitstatus

    VCSRepo.new(repo).checkout_branch('feat')
    _, output = run_get_task(repo)
    # workflow should receive correctly parsed arguments
    assert_includes output, 'foo bar baz'
    # second invocation should also appear in output
    assert_includes output, 'qux quux'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_setup_script_receives_env_vars
    skip 'Codex setup tests are intended for Linux cloud environments' if windows?

    repo, remote = setup_repo(:git)
    create_workflow(repo, 'envgen', <<~SH)
      #!/bin/sh
      echo '@agents-setup FOO=BAR'
    SH

    agents_dir = File.join(repo, '.agents')
    FileUtils.mkdir_p(agents_dir)
    result_file = File.join(repo, 'setup_result')
    File.write(File.join(agents_dir, 'codex-setup'), <<~SH)
      #!/bin/sh
      echo $FOO > #{result_file}
    SH
    File.chmod(0o755, File.join(agents_dir, 'codex-setup'))

    prompt = <<~PROMPT
      Prepare env.
      /envgen
      Done.
    PROMPT
    status, = run_agent_task(repo, branch: 'feat', prompt: prompt, push_to_remote: false)
    # agent-task should succeed before running setup
    assert_equal 0, status.exitstatus

    VCSRepo.new(repo).checkout_branch('feat')

    clone = Dir.mktmpdir('ah_clone')
    FileUtils.cp_r(File.join(RepoTestHelper::ROOT, '.'), clone)

    stub_dir = Dir.mktmpdir('stubs')
    File.write(File.join(stub_dir, 'sudo'), <<~SH)
      #!/bin/sh
      exit 0
    SH
    File.chmod(0o755, File.join(stub_dir, 'sudo'))
    File.write(File.join(stub_dir, 'mv'), <<~SH)
      #!/bin/sh
      exit 0
    SH
    File.chmod(0o755, File.join(stub_dir, 'mv'))

    path_env = ENV.fetch('PATH', nil)
    env = { 'PATH' => "#{stub_dir}:#{path_env}", 'HOME' => Dir.mktmpdir('home'), 'TEST_MODE' => '1' }
    IO.popen(env, [File.join(clone, 'codex-setup')], chdir: repo, &:read)

    # setup script from repo should capture env variable
    assert_equal 'BAR', File.read(result_file).strip
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    FileUtils.remove_entry(clone) if defined?(clone) && clone && File.exist?(clone)
    FileUtils.remove_entry(stub_dir) if defined?(stub_dir) && stub_dir && File.exist?(stub_dir)
    FileUtils.rm_rf('/tmp/agent-harbor')
  end
end

class WorkflowDiagnosticsTest < Minitest::Test
  include RepoTestHelper

  def create_workflow(repo, name, content)
    dir = File.join(repo, '.agents', 'workflows')
    FileUtils.mkdir_p(dir)
    path = File.join(dir, name)
    File.write(path, content)
    File.chmod(0o755, path)
    path
  end

  def test_unknown_workflow_command
    repo, remote = setup_repo(:git)
    at = AgentTasks.new(repo)
    _, _, diagnostics = at.process_workflows(<<~PROMPT)
      /missing
      Trailing text
    PROMPT
    # should report missing workflow command
    assert_includes diagnostics, "Unknown workflow command '/missing'"
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_conflicting_env_assignments
    repo, remote = setup_repo(:git)
    create_workflow(repo, 'a', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR=1'
    SH
    create_workflow(repo, 'b', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR=2'
    SH
    at = AgentTasks.new(repo)
    _, _, diagnostics = at.process_workflows(<<~PROMPT)
      /a
      /b
    PROMPT
    # should detect conflicting variable assignments
    assert_includes diagnostics, 'Conflicting assignment for VAR'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_assignment_with_appends
    repo, remote = setup_repo(:git)
    create_workflow(repo, 'set', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR=base'
    SH
    create_workflow(repo, 'add', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR+=extra'
    SH
    at = AgentTasks.new(repo)
    _, env1, diagnostics1 = at.process_workflows(<<~PROMPT)
      /set
      /add
    PROMPT
    # direct assignment followed by append should combine values
    assert_empty diagnostics1
    assert_equal 'base,extra', env1['VAR']

    _, env2, diagnostics2 = at.process_workflows(<<~PROMPT)
      /add
      /set
    PROMPT
    # append before assignment should yield the same result
    assert_empty diagnostics2
    assert_equal 'base,extra', env2['VAR']

    create_workflow(repo, 'set_dup', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR=base'
    SH
    _, env3, diagnostics3 = at.process_workflows(<<~PROMPT)
      /set
      /set_dup
      /add
    PROMPT
    # duplicate direct assignment should not cause diagnostics
    assert_empty diagnostics3
    assert_equal 'base,extra', env3['VAR']
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_append_only_combines_values
    repo, remote = setup_repo(:git)
    create_workflow(repo, 'add1', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR+=one'
    SH
    create_workflow(repo, 'add2', <<~SH)
      #!/bin/sh
      echo '@agents-setup VAR+=two'
    SH
    at = AgentTasks.new(repo)
    _, env1, diagnostics1 = at.process_workflows(<<~PROMPT)
      /add1
      /add2
    PROMPT
    # multiple append directives should accumulate
    assert_empty diagnostics1
    assert_equal 'one,two', env1['VAR']

    _, env2, diagnostics2 = at.process_workflows(<<~PROMPT)
      /add1
      /add1
    PROMPT
    # duplicate append values should be deduplicated
    assert_empty diagnostics2
    assert_equal 'one', env2['VAR']
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_workflow_command_failure_reports_stderr
    repo, remote = setup_repo(:git)
    create_workflow(repo, 'fail', <<~SH)
      #!/bin/sh
      echo boom >&2
      exit 1
    SH
    at = AgentTasks.new(repo)
    _, _, diagnostics = at.process_workflows(<<~PROMPT)
      Some text
      /fail
    PROMPT
    # stderr from failed command should appear in diagnostics
    assert(diagnostics.any? { |l| l.start_with?('$ fail') && l.include?('boom') })
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end
end
