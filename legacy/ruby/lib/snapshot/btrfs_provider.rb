# frozen_string_literal: true

require 'shellwords'
require 'json'
require 'socket'

module Snapshot
  # Btrfs subvolume snapshot implementation
  class BtrfsProvider < Provider
    DAEMON_SOCKET_PATH = '/tmp/agent-harbor/ah-fs-snapshots-daemon'

    def self.available?(path)
      # Btrfs is only available on Linux in this implementation
      return false unless RUBY_PLATFORM.include?('linux')

      system('which', 'btrfs', out: File::NULL, err: File::NULL) &&
        fs_type(path) == 'btrfs'
    end

    def create_workspace(dest)
      # Try to use daemon if available for privileged operations
      if daemon_available?
        result = create_snapshot_via_daemon(@repo_path, dest)
        if result && result[:success]
          return result[:path] || dest
        elsif result && result[:success] == false
          # Daemon operation failed - don't fall back, raise error to make test fail
          raise "Daemon Btrfs snapshot operation failed: #{result[:error]}"
        end
        # If result is nil (communication error), fall back to direct execution
      end

      # Fallback to direct execution
      run('btrfs', 'subvolume', 'snapshot', @repo_path, dest)
      dest
    end

    def cleanup_workspace(dest)
      # Only try to delete if the path exists
      return unless File.exist?(dest)

      # Try to use daemon if available for privileged operations
      if daemon_available?
        result = delete_snapshot_via_daemon(dest)
        if result && result[:success]
          return
        elsif result && result[:success] == false
          # Daemon operation failed - don't fall back, raise error to make test fail
          raise "Daemon Btrfs delete operation failed: #{result[:error]}"
        end
        # If result is nil (communication error), fall back to direct execution
      end

      # Fallback to direct execution
      # Delete recursively in case there are nested subvolumes
      run('btrfs', 'subvolume', 'delete', '-R', dest)
    end

    def self.fs_type(path)
      `stat -f -c %T #{Shellwords.escape(path)}`.strip
    end
    private_class_method :fs_type

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

    def create_snapshot_via_daemon(source, destination)
      request = {
        'command' => 'snapshot',
        'filesystem' => 'btrfs',
        'source' => source,
        'destination' => destination
      }

      response = send_daemon_request(request)
      return nil unless response

      {
        success: response['success'],
        path: response['path'] || destination,
        error: response['error']
      }
    end

    def delete_snapshot_via_daemon(target)
      request = {
        'command' => 'delete',
        'filesystem' => 'btrfs',
        'target' => target
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
