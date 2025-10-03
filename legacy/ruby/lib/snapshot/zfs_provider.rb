# frozen_string_literal: true

require 'shellwords'
require 'json'
require 'socket'

module Snapshot
  # ZFS snapshot implementation
  class ZfsProvider < Provider
    DAEMON_SOCKET_PATH = '/tmp/agent-harbor/ah-fs-snapshots-daemon'

    def self.available?(path)
      # ZFS is only available on Linux in this implementation
      return false unless RUBY_PLATFORM.include?('linux')

      system('which', 'zfs', out: File::NULL, err: File::NULL) &&
        fs_type(path) == 'zfs'
    end

    def create_workspace(dest)
      # Validate destination path
      validate_destination_path(dest)

      dataset = dataset_for(@repo_path)
      raise 'ZFS dataset not found' unless dataset

      tag = "agent#{Process.pid}_#{Thread.current.object_id}_#{Time.now.to_f.to_s.gsub('.', '_')}"
      snapshot = "#{dataset}@#{tag}"
      clone = "#{dataset}-clone-#{tag}"
      run('zfs', 'snapshot', snapshot)

      # Try to create clone using daemon if available
      if daemon_available?
        result = create_clone_via_daemon(snapshot, clone)
        if result && result[:success] && result[:mountpoint] && Dir.exist?(result[:mountpoint])
          # Clone created successfully via daemon
          FileUtils.mkdir_p(File.dirname(dest))
          # Remove existing file/directory if it exists, then create symlink
          FileUtils.rm_rf(dest) if File.exist?(dest) || File.symlink?(dest)
          File.symlink(result[:mountpoint], dest)
          return dest
        elsif result && result[:success] == false
          # Daemon operation failed - don't fall back, raise error to make test fail
          raise "Daemon ZFS clone operation failed: #{result[:error]}"
        end
        # If result is nil (communication error), fall back to direct execution
      end

      # Fallback to direct zfs clone (may require sudo or fail)
      clone_success = system('zfs', 'clone', snapshot, clone)
      unless clone_success
        # Check if the clone was actually created despite the error
        clone_exists = system('zfs', 'list', clone, out: File::NULL, err: File::NULL)
        raise("Command failed: zfs clone #{snapshot} #{clone}") unless clone_exists
      end

      # ZFS clone is created, now check if it's mounted and accessible
      actual_mountpoint = `zfs get -H -o value mountpoint #{clone} 2>/dev/null`.strip

      # Check if the clone is mounted and accessible
      if actual_mountpoint != 'none' && actual_mountpoint != 'legacy' && Dir.exist?(actual_mountpoint)
        repo_has_readme = File.exist?(File.join(@repo_path, 'README.md'))
        clone_has_readme = File.exist?(File.join(actual_mountpoint, 'README.md'))

        if repo_has_readme == clone_has_readme
          # Clone appears to be properly mounted and accessible
          # Create a symlink from dest to the actual mountpoint
          FileUtils.mkdir_p(File.dirname(dest))
          # Remove existing file/directory if it exists, then create symlink
          FileUtils.rm_rf(dest) if File.exist?(dest) || File.symlink?(dest)
          File.symlink(actual_mountpoint, dest)
          return dest
        end
      end

      # Clone is not properly accessible - this indicates ZFS mounting permissions issue
      raise 'ZFS clone created but not accessible - check ZFS mounting permissions ' \
            'or start the AH filesystem snapshots daemon with `just legacy-start-ah-fs-snapshots-daemon`'
    end

    def cleanup_workspace(dest)
      # Find and destroy ZFS datasets
      actual_path = begin
        File.realpath(dest)
      rescue StandardError
        dest
      end

      dataset = dataset_for(actual_path)
      dataset ||= dataset_for(dest)

      if dataset
        # Try to use daemon for privileged cleanup first
        if daemon_available?
          result = delete_dataset_via_daemon(dataset)
          if result && result[:success]
            # Remove the symlink if it exists
            File.unlink(dest) if File.symlink?(dest)
            return
          elsif result && result[:success] == false
            # Daemon operation failed - don't fall back, raise error to make test fail
            raise "Daemon ZFS delete operation failed: #{result[:error]}"
          end
          # If result is nil (communication error), fall back to direct execution
        end

        # Fallback to direct execution
        begin
          run('zfs', 'destroy', '-r', dataset)
        rescue StandardError
          nil
        end
      end

      # Remove the symlink if it exists
      File.unlink(dest) if File.symlink?(dest)
    end

    private

    def daemon_available?
      File.socket?(DAEMON_SOCKET_PATH)
    end

    def send_daemon_request(request)
      return nil unless daemon_available?

      UNIXSocket.open(DAEMON_SOCKET_PATH) do |socket|
        socket.puts(request.to_json)
        response = socket.gets
        return nil unless response

        JSON.parse(response)
      end
    rescue StandardError
      nil
    end

    def create_clone_via_daemon(snapshot, clone)
      request = {
        'command' => 'clone',
        'filesystem' => 'zfs',
        'snapshot' => snapshot,
        'clone' => clone
      }

      response = send_daemon_request(request)
      return nil unless response

      {
        success: response['success'],
        mountpoint: response['mountpoint'],
        error: response['error']
      }
    end

    def self.fs_type(path)
      `stat -f -c %T #{Shellwords.escape(path)}`.strip
    end
    private_class_method :fs_type

    private

    def validate_destination_path(dest)
      # Check if the destination path can be created as a directory
      begin
        # Try to create the parent directory to validate the path
        parent_dir = File.dirname(dest)
        FileUtils.mkdir_p(parent_dir)
        # Clean up the test directory
        Dir.rmdir(parent_dir) if Dir.exist?(parent_dir) && Dir.empty?(parent_dir)
      rescue StandardError => e
        raise "Invalid destination path: #{dest} (#{e.message})"
      end

      # Additional validation: ensure it's not trying to create in system directories
      # that should not be used for workspaces
      invalid_paths = ['/dev', '/proc', '/sys', '/run']
      return unless invalid_paths.any? { |invalid| dest.start_with?(invalid) }

      raise "Cannot create workspace in system directory: #{dest}"
    end

    def dataset_for(path)
      list = `zfs list -H -o name,mountpoint 2>/dev/null`
      candidates = list.lines.map(&:split)
                       .select do |_name, mount|
                         mount != 'none' && mount != 'legacy' && mount != '/' && path.start_with?(mount)
                       end

      # Find the dataset with the longest mountpoint that actually contains the path
      best = candidates.max_by { |_, mount| mount.length }

      # Only return the dataset if the path actually exists within it
      return unless best && File.exist?(path)

      best.first
    end

    def delete_dataset_via_daemon(dataset)
      request = {
        'command' => 'delete',
        'filesystem' => 'zfs',
        'target' => dataset
      }

      response = send_daemon_request(request)
      return nil unless response

      {
        success: response['success'],
        error: response['error']
      }
    end

    def run(*cmd)
      system(*cmd) || raise("Command failed: #{cmd.join(' ')}")
    end
  end
end
