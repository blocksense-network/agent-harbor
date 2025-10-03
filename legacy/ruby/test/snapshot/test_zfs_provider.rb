# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative '../test_helper'
require_relative 'provider_shared_behavior'
require_relative 'provider_quota_test_behavior'
require_relative 'filesystem_space_utils'
require 'snapshot/provider'

# Comprehensive tests for ZFS provider combining generic and specific tests
class TestZfsProvider < Minitest::Test
  include RepoTestHelper
  include FilesystemSpaceUtils
  include ProviderSharedBehavior
  include ProviderQuotaTestBehavior

  def setup
    skip 'ZFS tests only run on Linux' unless linux?
    skip 'ZFS tools not available' unless system('which', 'zfs', out: File::NULL, err: File::NULL)

    # Check for pre-created test filesystem
    @cache_dir = File.expand_path('~/.cache/agent-harbor')
    @pool_name = 'AH_test_zfs'
    @dataset_name = "#{@pool_name}/test_dataset"

    unless zfs_pool_exists?(@pool_name)
      skip "ZFS test pool '#{@pool_name}' not found. " \
           "Run 'just create-test-filesystems' to set up reusable test filesystems."
    end

    unless zfs_dataset_exists?(@dataset_name)
      skip "ZFS test dataset '#{@dataset_name}' not found. " \
           "Run 'just create-test-filesystems' to set up reusable test filesystems."
    end

    # Use the pre-created dataset for testing
    @repo_dir = zfs_get_mountpoint(@dataset_name)
    unless @repo_dir && Dir.exist?(@repo_dir)
      skip "ZFS test dataset '#{@dataset_name}' is not mounted. " \
           "Run 'just create-test-filesystems' to set up reusable test filesystems."
    end

    # Initialize the repo with test content if it doesn't exist
    readme_path = File.join(@repo_dir, 'README.md')
    return if File.exist?(readme_path)

    File.write(readme_path, 'test repo content')

    # Tests must be safe for concurrent execution. Each test method uses unique snapshot names
    # based on process ID and timestamp to avoid conflicts between parallel test runs.
    # The pre-created filesystem provides isolation through ZFS snapshots/clones.
  end

  def teardown
    # Cleanup any snapshots created during testing
    cleanup_test_snapshots
  end

  # === ZFS-specific helper methods ===

  def zfs_pool_exists?(pool_name)
    system('zpool', 'list', pool_name, out: File::NULL, err: File::NULL)
  end

  def zfs_dataset_exists?(dataset_name)
    system('zfs', 'list', dataset_name, out: File::NULL, err: File::NULL)
  end

  def zfs_get_mountpoint(dataset_name)
    output = `zfs list -H -o mountpoint #{dataset_name} 2>/dev/null`.strip
    output.empty? ? nil : output
  end

  def cleanup_test_snapshots
    # Destroy any snapshots created by this test run
    # We identify them by patterns that include the process ID or agent
    pid = Process.pid
    snapshots_to_destroy = []

    # Find snapshots with process ID
    `zfs list -H -o name -t snapshot 2>/dev/null | grep "test.*#{pid}"`.each_line do |snapshot|
      snapshots_to_destroy << snapshot.strip!
    end

    # Find agent snapshots (from ZFS provider) - both old and new formats
    `zfs list -H -o name -t snapshot 2>/dev/null | grep "agent.*#{pid}"`.each_line do |snapshot|
      snapshots_to_destroy << snapshot.strip!
    end

    # Also clean up any agent snapshots without PID (legacy)
    `zfs list -H -o name -t snapshot 2>/dev/null | grep "^AH_test_zfs/test_dataset@agent[0-9]"`
      .each_line do |snapshot|
      snapshots_to_destroy << snapshot.strip!
    end

    # Destroy the snapshots (and their clones)
    snapshots_to_destroy.uniq.each do |snapshot|
      system('zfs', 'destroy', '-r', snapshot, out: File::NULL, err: File::NULL)
    end
  end

  def get_dataset_for_path(path)
    # Get the ZFS dataset that contains the given path
    output = `zfs list -H -o name,mountpoint 2>/dev/null`
    output.lines.each do |line|
      name, mountpoint = line.strip.split("\t")
      return name if path.start_with?(mountpoint)
    end
    nil
  end

  # === Generic test implementation ===

  private

  def create_test_provider
    Snapshot::ZfsProvider.new(@repo_dir)
  end

  def provider_skip_reason
    # ZFS provider needs the daemon for reliable concurrent operations
    # The daemon provides proper isolation and mounting for concurrent ZFS clones
    daemon_socket = '/tmp/agent-harbor/ah-fs-snapshots-daemon'

    # Check if socket exists and we can actually connect to it
    if File.socket?(daemon_socket)
      begin
        # Try to connect to test if daemon is actually running
        UNIXSocket.open(daemon_socket) do |socket|
          # Send a test ping
          socket.puts({ 'command' => 'ping' }.to_json)
          response = socket.gets
          if response
            resp = begin
              JSON.parse(response.strip)
            rescue StandardError
              nil
            end
            return nil if resp && resp['success'] # Daemon is working
          end
        end
      rescue StandardError
        # Daemon not responding, socket is stale
      end
    end

    'ZFS daemon not available for concurrent operations'
  end

  def expected_max_creation_time
    5.0 # ZFS snapshots should be very fast
  end

  def expected_max_cleanup_time
    3.0 # ZFS cleanup should be fast
  end

  def expected_concurrent_count
    5 # ZFS handles concurrency well
  end

  def supports_space_efficiency_test?
    true # ZFS supports CoW
  end

  def measure_space_usage
    zfs_pool_used_space(@pool_name)
  end

  def expected_max_space_usage
    1024 * 1024 # 1MB for ZFS metadata
  end

  def create_workspace_destination(suffix = nil)
    # Use process ID in directory name for concurrent test safety
    pid = Process.pid
    timestamp = Time.now.to_i
    base_name = suffix ? "zfs_workspace_#{suffix}_#{pid}_#{timestamp}" : "zfs_workspace_#{pid}_#{timestamp}"
    Dir.mktmpdir(base_name)
  end

  def cleanup_test_workspace(workspace_dir)
    FileUtils.rm_rf(workspace_dir) if workspace_dir && File.exist?(workspace_dir)
  end

  def test_repo_content
    'test repo content'
  end

  def verify_cleanup_behavior(workspace_dir, _result_path)
    # For ZFS provider, cleanup should remove the symlink but the directory may remain
    # (depending on whether the clone unmounting succeeded)
    # The important thing is that the symlink is gone
    refute File.symlink?(workspace_dir), 'Workspace symlink should not exist after cleanup'
  end

  def provider_class_name
    'ZfsProvider'
  end

  def expected_provider_class
    Snapshot::ZfsProvider
  end

  def expected_native_creation_time
    5.0 # ZFS snapshots are fast
  end

  def expected_native_cleanup_time
    3.0 # ZFS cleanup is fast
  end

  # === Quota test implementation ===

  def supports_quota_testing?
    true # ZFS supports quota testing
  end

  def setup_quota_environment
    # Set a quota on the dataset
    dataset = get_dataset_for_path(@repo_dir)
    success = system('zfs', 'set', 'quota=10M', dataset, out: File::NULL, err: File::NULL)
    raise "Failed to set quota on #{dataset}" unless success
  end

  def cleanup_quota_environment
    # Quota cleanup handled by pool destruction
  end

  def verify_quota_behavior(quota_exceeded)
    # ZFS should enforce quotas strictly
    assert quota_exceeded, 'ZFS should have enforced the 10MB quota limit'
  end

  # Override the quota test to set quota on the clone after creation
  def test_provider_quota_limits
    return skip "Quota testing not supported for #{provider_class_name}" unless supports_quota_testing?

    provider = create_test_provider
    workspace_dir = create_workspace_destination('quota_test')

    begin
      # Create workspace first
      result_path = provider.create_workspace(workspace_dir)

      # Now set quota on the clone dataset
      clone_dataset = get_dataset_for_path(result_path)
      success = system('zfs', 'set', 'quota=10M', clone_dataset, out: File::NULL, err: File::NULL)
      raise "Failed to set quota on #{clone_dataset}" unless success

      # Try to exceed quota
      large_file = File.join(result_path, 'large_file.dat')
      quota_exceeded = false

      begin
        File.write(large_file, 'x' * quota_test_size)
      rescue Errno::ENOSPC, Errno::EDQUOT
        quota_exceeded = true
      end

      # Verify quota behavior
      verify_quota_behavior(quota_exceeded)
    ensure
      # Clean up quota
      system('zfs', 'set', 'quota=none', clone_dataset, out: File::NULL, err: File::NULL) if clone_dataset
      provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
    end
  end

  public

  # === ZFS-specific tests ===

  def zfs_mounting_available?
    # Check if daemon is available and responding
    daemon_socket = '/tmp/agent-harbor/ah-fs-snapshots-daemon'
    if File.socket?(daemon_socket)
      begin
        # Try to connect to test if daemon is actually running
        UNIXSocket.open(daemon_socket) do |socket|
          socket.puts({ 'command' => 'ping' }.to_json)
          response = socket.gets
          if response
            resp = begin
              JSON.parse(response.strip)
            rescue StandardError
              nil
            end
            return true if resp && resp['success'] # Daemon is working
          end
        end
      rescue StandardError
        # Daemon not responding, socket is stale
      end
    end

    # Test if ZFS mounting works for regular users by creating a test clone
    test_snapshot = "#{@dataset_name}@mount_test_#{Process.pid}_#{Time.now.to_i}"
    test_clone = "#{@dataset_name}-clone-mount_test_#{Process.pid}_#{Time.now.to_i}"

    begin
      # Create a test snapshot and clone
      system('zfs', 'snapshot', test_snapshot, out: File::NULL, err: File::NULL)
      clone_success = system('zfs', 'clone', test_snapshot, test_clone, out: File::NULL, err: File::NULL)

      if clone_success
        # Check if the clone is mounted and accessible
        actual_mountpoint = `zfs get -H -o value mountpoint #{test_clone} 2>/dev/null`.strip
        accessible = actual_mountpoint != 'none' && actual_mountpoint != 'legacy' && Dir.exist?(actual_mountpoint)
        return accessible
      end
    rescue StandardError
      return false
    ensure
      # Cleanup
      begin
        system('zfs', 'destroy', '-r', test_clone, out: File::NULL, err: File::NULL)
      rescue StandardError
        nil
      end
      begin
        system('zfs', 'destroy', test_snapshot, out: File::NULL, err: File::NULL)
      rescue StandardError
        nil
      end
    end
    false
  end

  def test_zfs_snapshot_and_clone_operations
    unless zfs_mounting_available?
      skip 'ZFS mounting not available for regular users. ' \
           'Start the daemon with `just legacy-start-ah-fs-snapshots-daemon`'
    end
    provider = Snapshot::ZfsProvider.new(@repo_dir)
    workspace_dir = create_workspace_destination('clone_test')

    begin
      # Create workspace using ZFS snapshot/clone
      start_time = Time.now
      result_path = provider.create_workspace(workspace_dir)
      creation_time = Time.now - start_time

      # Verify workspace was created
      assert File.exist?(result_path)
      assert File.exist?(File.join(result_path, 'README.md'))

      # Verify CoW behavior - changes in workspace don't affect original
      File.write(File.join(result_path, 'workspace_file.txt'), 'workspace content')
      refute File.exist?(File.join(@repo_dir, 'workspace_file.txt'))

      # Verify original file content is accessible
      assert_equal 'test repo content', File.read(File.join(result_path, 'README.md'))

      # Test performance - snapshot creation should be fast (< 5 seconds for small repos)
      assert creation_time < 5.0, "Snapshot creation took #{creation_time}s, expected < 5s"

      # Test cleanup
      start_time = Time.now
      provider.cleanup_workspace(workspace_dir)
      cleanup_time = Time.now - start_time

      # Verify cleanup performance
      assert cleanup_time < 3.0, "Cleanup took #{cleanup_time}s, expected < 3s"
    ensure
      provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
      begin
        FileUtils.rm_rf(workspace_dir)
      rescue StandardError
        nil
      end
    end
  end

  def test_zfs_error_conditions
    unless zfs_mounting_available?
      skip 'ZFS mounting not available for regular users. ' \
           'Start the daemon with `just legacy-start-ah-fs-snapshots-daemon`'
    end
    provider = Snapshot::ZfsProvider.new(@repo_dir)

    # Test with invalid destination (system directory)
    assert_raises(RuntimeError) do
      provider.create_workspace('/dev/test/workspace')
    end

    # Test cleanup of non-existent workspace
    begin
      provider.cleanup_workspace('/non/existent/path')
      pass 'Cleanup of non-existent workspace should not raise'
    rescue StandardError => e
      flunk "Cleanup of non-existent workspace raised: #{e.message}"
    end
  end

  def test_daemon_success_error_reporting
    # Test that daemon operations work when daemon is available
    # This test will be skipped if daemon is not running
    daemon_socket = '/tmp/agent-harbor/ah-fs-snapshots-daemon'
    if File.socket?(daemon_socket)
      begin
        # Try to connect to test if daemon is actually running
        UNIXSocket.open(daemon_socket) do |socket|
          socket.puts({ 'command' => 'ping' }.to_json)
          response = socket.gets
          if response
            resp = begin
              JSON.parse(response.strip)
            rescue StandardError
              nil
            end
            skip 'Daemon ping failed' unless resp && resp['success']
          else
            skip 'Daemon not responding to ping'
          end
        end
      rescue StandardError
        skip 'Daemon not available'
      end
    else
      skip 'Daemon not available'
    end

    provider = Snapshot::ZfsProvider.new(@repo_dir)
    workspace_dir = create_workspace_destination('daemon_test')

    begin
      # Test successful daemon operation
      result_path = provider.create_workspace(workspace_dir)

      # Verify workspace was created successfully
      assert File.exist?(result_path), 'Workspace should be created'
      assert File.exist?(File.join(result_path, 'README.md')), 'Workspace should contain files'

      # Verify daemon operation succeeded (no exception should be raised)
      pass 'Daemon operation succeeded as expected'

      provider.cleanup_workspace(workspace_dir)
    ensure
      provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
      begin
        FileUtils.rm_rf(workspace_dir)
      rescue StandardError
        nil
      end
    end
  end

  def daemon_available?
    socket_path = '/tmp/agent-harbor/ah-fs-snapshots-daemon'
    File.socket?(socket_path)
  end

  def test_zfs_space_usage_efficiency
    provider = Snapshot::ZfsProvider.new(@repo_dir)
    workspace_dir = create_workspace_destination('space_test')

    begin
      # Measure space before snapshot
      space_before = zfs_pool_used_space(@pool_name)

      # Create workspace
      provider.create_workspace(workspace_dir)

      # Measure space after snapshot (should be minimal due to CoW)
      space_after = zfs_pool_used_space(@pool_name)
      space_used = space_after - space_before

      # Snapshot should use minimal space (less than 1MB for metadata)
      assert space_used < 1024 * 1024, "Snapshot used #{space_used} bytes, expected < 1MB"
    rescue RuntimeError => e
      # Skip if ZFS operations fail due to permissions or setup issues
      skip "ZFS space test failed: #{e.message}"
    ensure
      begin
        provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
      rescue StandardError
        # Ignore cleanup errors
      end
      begin
        FileUtils.rm_rf(workspace_dir)
      rescue StandardError
        nil
      end
    end
  end

  # ZFS-specific helper methods
end
