# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative 'test_helper'

module StartWorkCases
  def test_start_work_task_recording
    RepoTestHelper::START_WORK_BINARIES.each do |sb|
      repo, remote = setup_repo(self.class::VCS_TYPE)

      # Test initial task recording with branch name
      status, = run_start_work(repo, tool: sb, task_description: 'initial task', branch_name: 'feat')
      assert_equal 0, status.exitstatus

      # Verify task file was created
      tasks_dir = Dir[File.join(repo, '.agents', 'tasks', '*', '*')].first
      refute_nil tasks_dir, 'Task directory should be created'

      files = Dir.children(tasks_dir)
      assert_equal 1, files.length, 'Should have exactly one task file'

      task_file_path = File.join(tasks_dir, files.first)
      content = File.read(task_file_path)
      assert_includes content, 'initial task'

      # Switch to the created branch and test follow-up task
      VCSRepo.new(repo).checkout_branch('feat')
      status2, = run_start_work(repo, tool: sb, task_description: 'follow-up task')
      assert_equal 0, status2.exitstatus

      # Verify follow-up task was appended
      updated_content = File.read(task_file_path)
      assert_includes updated_content, 'initial task'
      assert_includes updated_content, 'follow-up task'
      assert_includes updated_content, '--- FOLLOW UP TASK ---'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_start_work_from_parent_directory
    RepoTestHelper::START_WORK_BINARIES.each do |sb|
      repo, remote = setup_repo(self.class::VCS_TYPE)

      # Create initial task
      status, = run_start_work(repo, tool: sb, task_description: 'task in repo', branch_name: 'feat')
      assert_equal 0, status.exitstatus

      # Move repo to parent directory
      outer = Dir.mktmpdir('outer')
      FileUtils.mv(repo, File.join(outer, 'repo'))
      repo = File.join(outer, 'repo')

      # Test task recording from parent directory
      VCSRepo.new(repo).checkout_branch('feat')
      status2, = run_start_work(outer, tool: sb, task_description: 'task from parent')
      assert_equal 0, status2.exitstatus

      # Verify task was recorded
      tasks_dir = Dir[File.join(repo, '.agents', 'tasks', '*', '*')].first
      files = Dir.children(tasks_dir)
      task_file_path = File.join(tasks_dir, files.first)
      content = File.read(task_file_path)
      assert_includes content, 'task in repo'
      assert_includes content, 'task from parent'
    ensure
      FileUtils.remove_entry(outer) if outer && File.exist?(outer)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_start_work_multiple_repos
    RepoTestHelper::START_WORK_BINARIES.each do |sb|
      repo_a, remote_a = setup_repo(self.class::VCS_TYPE)
      repo_b, remote_b = setup_repo(self.class::VCS_TYPE)

      # Set up initial tasks in both repos
      status_a, = run_start_work(repo_a, tool: sb, task_description: 'task in repo A', branch_name: 'feat-a')
      assert_equal 0, status_a.exitstatus

      status_b, = run_start_work(repo_b, tool: sb, task_description: 'task in repo B', branch_name: 'feat-b')
      assert_equal 0, status_b.exitstatus

      # Move both repos to parent directory
      outer = Dir.mktmpdir('outer')
      FileUtils.mv(repo_a, File.join(outer, 'repo_a'))
      FileUtils.mv(repo_b, File.join(outer, 'repo_b'))
      repo_a = File.join(outer, 'repo_a')
      repo_b = File.join(outer, 'repo_b')

      # Checkout branches in both repos
      VCSRepo.new(repo_a).checkout_branch('feat-a')
      VCSRepo.new(repo_b).checkout_branch('feat-b')

      # Test task recording from parent directory affecting both repos
      status, = run_start_work(outer, tool: sb, task_description: 'task for both repos')
      assert_equal 0, status.exitstatus

      # Verify tasks were recorded in both repos
      [repo_a, repo_b].each_with_index do |repo, index|
        tasks_dir = Dir[File.join(repo, '.agents', 'tasks', '*', '*')].first
        files = Dir.children(tasks_dir)
        task_file_path = File.join(tasks_dir, files.first)
        content = File.read(task_file_path)

        expected_initial = index.zero? ? 'task in repo A' : 'task in repo B'
        assert_includes content, expected_initial
        assert_includes content, 'task for both repos'
      end
    ensure
      FileUtils.remove_entry(outer) if outer && File.exist?(outer)
      FileUtils.remove_entry(remote_a) if remote_a && File.exist?(remote_a)
      FileUtils.remove_entry(remote_b) if remote_b && File.exist?(remote_b)
    end
  end

  def test_start_work_error_conditions
    RepoTestHelper::START_WORK_BINARIES.each do |sb|
      repo, remote = setup_repo(self.class::VCS_TYPE)

      # Test error when branch name is missing for initial task
      status, output = run_start_work(repo, tool: sb, task_description: 'task without branch')
      assert_equal 1, status.exitstatus
      assert_includes output, '--branch-name is required when not on an agent branch'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end
end

class StartWorkGitTest < Minitest::Test
  include RepoTestHelper
  include StartWorkCases

  VCS_TYPE = :git
end
