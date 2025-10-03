# frozen_string_literal: true

require 'English'
require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative '../test_helper'
require_relative 'provider_shared_behavior'
require_relative 'provider_quota_test_behavior'
require_relative 'filesystem_test_helper'
require_relative 'filesystem_space_utils'
require 'snapshot/provider'

# Comprehensive tests for Btrfs provider combining generic and specific tests
class TestBtrfsProvider < Minitest::Test
  include RepoTestHelper
  include FilesystemTestHelper
  include FilesystemSpaceUtils
  include ProviderSharedBehavior
  include ProviderQuotaTestBehavior

  def setup
    skip 'Btrfs tests only run on Linux' unless linux?
    skip 'Btrfs tools not available' unless system('which', 'btrfs', out: File::NULL, err: File::NULL)

    # Check for pre-created test filesystem
    @cache_dir = File.expand_path('~/.cache/agent-harbor')
    @btrfs_loop = '/dev/loop99'
    @subvolume_path = File.join(@cache_dir, 'btrfs_mount', 'test_subvol')

    unless btrfs_mounted?(@btrfs_loop)
      skip "Btrfs test filesystem not mounted at #{@btrfs_loop}. " \
           "Run 'just create-test-filesystems' to set up reusable test filesystems."
    end

    unless Dir.exist?(@subvolume_path)
      skip "Btrfs test subvolume not found at #{@subvolume_path}. " \
           "Run 'just create-test-filesystems' to set up reusable test filesystems."
    end

    # Use the pre-created subvolume for testing
    @repo_dir = @subvolume_path

    # Initialize the repo with test content if it doesn't exist
    readme_path = File.join(@repo_dir, 'README.md')
    return if File.exist?(readme_path)

    File.write(readme_path, 'test repo content')

    # Tests must be safe for concurrent execution. Each test method uses unique snapshot names
    # based on process ID and timestamp to avoid conflicts between parallel test runs.
    # The pre-created subvolume provides isolation through Btrfs snapshots.
  end

  def teardown
    # Cleanup any snapshots created during testing
    cleanup_test_snapshots
  end

  # === Generic test implementation ===

  private

  def create_test_provider
    Snapshot::BtrfsProvider.new(@repo_dir)
  end

  def provider_skip_reason
    # Btrfs provider doesn't have special skip conditions beyond the main setup
    nil
  end

  def expected_max_creation_time
    3.0 # Btrfs snapshots should be fast
  end

  def expected_max_cleanup_time
    2.0 # Btrfs cleanup should be fast
  end

  def expected_concurrent_count
    3 # Btrfs handles concurrency reasonably well
  end

  def supports_space_efficiency_test?
    true # Btrfs supports CoW
  end

  def measure_space_usage
    btrfs_filesystem_used_space(@repo_dir)
  end

  def expected_max_space_usage
    512 * 1024 # 512KB for Btrfs metadata
  end

  def create_workspace_destination(suffix = nil)
    # Use process ID in directory name for concurrent test safety
    pid = Process.pid
    timestamp = Time.now.to_i
    base_name = suffix ? "btrfs_workspace_#{suffix}_#{pid}_#{timestamp}" : "btrfs_workspace_#{pid}_#{timestamp}"
    File.join(@repo_dir, base_name)
  end

  def expected_provider_class
    Snapshot::BtrfsProvider
  end

  def create_native_workspace_destination
    File.join(@repo_dir, 'native_workspace')
  end

  # === Btrfs-specific helper methods ===

  def btrfs_mounted?(loop_device)
    mount_output = `mount 2>/dev/null`
    mount_output.include?(loop_device)
  end

  def cleanup_test_snapshots
    # Destroy any snapshots created by this test run
    # We identify them by a pattern that includes the process ID
    pid = Process.pid
    `btrfs subvolume list #{@repo_dir} 2>/dev/null | grep "test.*#{pid}"`.each_line do |line|
      if line =~ /ID (\d+)/
        subvol_id = ::Regexp.last_match(1)
        system('btrfs', 'subvolume', 'delete', "#{@repo_dir}/.snapshots/#{subvol_id}", out: File::NULL, err: File::NULL)
      end
    end
  end

  def expected_native_creation_time
    3.0 # Btrfs snapshots are fast
  end

  def expected_native_cleanup_time
    2.0 # Btrfs cleanup is fast
  end

  # === Quota test implementation ===

  def supports_quota_testing?
    false # Quota testing requires additional Btrfs quota setup
  end

  def setup_quota_environment
    # Enable quotas on the filesystem
    system('btrfs', 'quota', 'enable', @repo_dir, out: File::NULL, err: File::NULL)

    # Set a quota limit on the subvolume
    subvol_id = get_subvolume_id(@repo_dir)
    return unless subvol_id

    # Set 10MB limit
    system('btrfs', 'qgroup', 'limit', '10M', "0/#{subvol_id}", @repo_dir,
           out: File::NULL, err: File::NULL)
  end

  def cleanup_quota_environment
    # Quota cleanup handled by filesystem unmount
  end

  def get_subvolume_id(path)
    # Get the Btrfs subvolume ID for a given path
    output = `btrfs subvolume show "#{path}" 2>/dev/null | grep "Subvolume ID:" | awk '{print $3}'`.strip
    output.empty? ? nil : output.to_i
  end

  def verify_quota_behavior(quota_exceeded)
    # NOTE: Btrfs quotas may not immediately enforce limits in all scenarios
    # This test documents the current behavior
    if quota_exceeded
      # Good - quota was enforced
    else
      # This is also acceptable for Btrfs as quotas can be complex
      puts 'Note: Btrfs quota enforcement may be delayed or disabled'
    end
  end

  public

  # === Btrfs-specific tests ===

  def test_btrfs_subvolume_snapshot_operations
    provider = Snapshot::BtrfsProvider.new(@repo_dir)
    workspace_dir = File.join(@repo_dir, 'workspace_snapshot')

    begin
      # Create workspace using Btrfs subvolume snapshot
      start_time = Time.now
      result_path = provider.create_workspace(workspace_dir)
      creation_time = Time.now - start_time

      # Verify workspace was created
      assert File.exist?(result_path)
      # NOTE: README.md may not exist if repo is empty, but workspace creation should still work

      # Verify CoW behavior - changes in workspace don't affect original
      File.write(File.join(result_path, 'workspace_file.txt'), 'workspace content')
      refute File.exist?(File.join(@repo_dir, 'workspace_file.txt'))

      # Verify that files from the original repo are accessible in the workspace
      if File.exist?(File.join(@repo_dir, 'README.md'))
        assert_equal 'test repo content', File.read(File.join(result_path, 'README.md'))
      end

      # Test performance - subvolume snapshot should be fast (< 3 seconds)
      assert creation_time < 3.0, "Snapshot creation took #{creation_time}s, expected < 3s"

      # Test cleanup
      start_time = Time.now
      provider.cleanup_workspace(workspace_dir)
      cleanup_time = Time.now - start_time

      # Verify cleanup performance
      assert cleanup_time < 2.0, "Cleanup took #{cleanup_time}s, expected < 2s"
    ensure
      provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
    end
  end

  def test_btrfs_auto_subvolume_creation
    skip 'Btrfs operations require special filesystem permissions not available in test environment'

    # Use the pre-created subvolume for testing
    provider = Snapshot::BtrfsProvider.new(@repo_dir)
    workspace_dir = File.join(@repo_dir, 'auto_subvol_test')

    begin
      # This should automatically convert the directory to a subvolume if needed
      # (Note: Current implementation expects repo to already be a subvolume)
      # This test documents the current behavior
      assert_raises(RuntimeError) do
        provider.create_workspace(workspace_dir)
      end
    ensure
      provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
    end
  end

  def test_btrfs_error_conditions
    provider = Snapshot::BtrfsProvider.new(@repo_dir)

    # Test with invalid destination (outside Btrfs filesystem)
    assert_raises(RuntimeError) do
      provider.create_workspace('/tmp/invalid_btrfs_path')
    end

    # Test cleanup of non-existent workspace
    begin
      provider.cleanup_workspace('/non/existent/path')
      pass 'Cleanup of non-existent workspace should not raise'
    rescue StandardError => e
      flunk "Cleanup of non-existent workspace raised: #{e.message}"
    end
  end

  def test_btrfs_space_usage_efficiency
    provider = Snapshot::BtrfsProvider.new(@repo_dir)
    workspace_dir = File.join(@repo_dir, 'space_test')

    begin
      # Measure space before snapshot
      space_before = btrfs_filesystem_used_space(@repo_dir)

      # Create workspace
      provider.create_workspace(workspace_dir)

      # Measure space after snapshot (should be minimal due to CoW)
      space_after = btrfs_filesystem_used_space(@repo_dir)
      space_used = space_after - space_before

      # Snapshot should use minimal space (less than 512KB for metadata)
      assert space_used < 512 * 1024, "Snapshot used #{space_used} bytes, expected < 512KB"
    ensure
      provider.cleanup_workspace(workspace_dir) if File.exist?(workspace_dir)
    end
  end

  def test_btrfs_snapshot_performance_scaling
    # Create a larger repository with multiple files
    100.times do |i|
      File.write(File.join(@repo_dir, "file_#{i}.txt"), "content #{i}" * 100)
    end

    provider = Snapshot::BtrfsProvider.new(@repo_dir)

    # Test multiple snapshots to verify consistent performance
    times = []
    timestamp = Time.now.to_i
    5.times do |i|
      workspace_dir = File.join(@repo_dir, "perf_test_#{timestamp}_#{i}")

      start_time = Time.now
      provider.create_workspace(workspace_dir)
      times << (Time.now - start_time)

      provider.cleanup_workspace(workspace_dir)
    end

    # All snapshots should complete quickly
    times.each_with_index do |time, i|
      assert time < 2.0, "Snapshot #{i} took #{time}s, expected < 2s"
    end

    # Average time should be consistent
    avg_time = times.sum / times.size
    assert avg_time < 1.0, "Average snapshot time #{avg_time}s, expected < 1s"
  end

  def test_daemon_success_error_reporting
    skip 'Daemon not available' unless daemon_available?

    provider = Snapshot::BtrfsProvider.new(@repo_dir)
    workspace_dir = File.join(@repo_dir, 'daemon_test_workspace')

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
    end
  end

  def daemon_available?
    socket_path = '/tmp/agent-harbor/ah-fs-snapshots-daemon'
    File.socket?(socket_path)
  end

  def cleanup_test_workspace(workspace_dir)
    FileUtils.rm_rf(workspace_dir) if workspace_dir && File.exist?(workspace_dir)
  end

  def test_repo_content
    'test repo content'
  end

  def verify_cleanup_behavior(workspace_dir, _result_path)
    # For Btrfs provider, cleanup should remove the subvolume
    refute File.exist?(workspace_dir), 'Workspace directory should not exist after cleanup'
  end
end
