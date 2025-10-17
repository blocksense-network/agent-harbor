# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require 'open3'
require_relative 'test_helper'

module GetTaskCases
  def test_get_task_after_start
    RepoTestHelper::AGENT_TASK_BINARIES.product(RepoTestHelper::GET_TASK_BINARIES).each do |ab, gb|
      repo, remote = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'feat', lines: ['my task'], push_to_remote: push_flag, tool: ab)
      # agent-task should succeed
      assert_equal 0, status.exitstatus
      VCSRepo.new(repo).checkout_branch('feat')
      status2, output = run_get_task(repo, tool: gb)
      # get-task should print the task description
      assert_equal 0, status2.exitstatus
      assert_includes output, 'my task'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_get_task_on_work_branch
    RepoTestHelper::AGENT_TASK_BINARIES.product(RepoTestHelper::GET_TASK_BINARIES).each do |ab, gb|
      repo, remote = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'feat', lines: ['follow task'], push_to_remote: push_flag, tool: ab)
      # agent-task should succeed
      assert_equal 0, status.exitstatus
      r = VCSRepo.new(repo)
      r.checkout_branch('feat')
      r.create_local_branch('work')
      status2, output = run_get_task(repo, tool: gb)
      # even after switching to a different work branch the task should be retrievable
      assert_equal 0, status2.exitstatus
      assert_includes output, 'follow task'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_get_task_from_parent_directory
    RepoTestHelper::AGENT_TASK_BINARIES.product(RepoTestHelper::GET_TASK_BINARIES).each do |ab, gb|
      repo, remote = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'feat', lines: ['outer task'], push_to_remote: push_flag, tool: ab)
      # agent-task should succeed
      assert_equal 0, status.exitstatus
      # Switch to the agent task branch so discovery can find it
      VCSRepo.new(repo).checkout_branch('feat')
      outer = Dir.mktmpdir('outer')
      FileUtils.mv(repo, File.join(outer, 'repo'))
      status2, output = run_get_task(outer, tool: gb)
      # get-task should succeed when launched from the parent directory
      assert_equal 0, status2.exitstatus
      # the output should contain the task description without directory hints
      assert_includes output, 'outer task'
      refute_includes output, 'In directory'
    ensure
      FileUtils.remove_entry(outer) if outer && File.exist?(outer)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_get_task_from_parent_directory_multiple_repos
    RepoTestHelper::AGENT_TASK_BINARIES.product(RepoTestHelper::GET_TASK_BINARIES).each do |ab, gb|
      repo_a, remote_a = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo_a, branch: 'feat', lines: ['task a'], push_to_remote: push_flag, tool: ab)
      # first repo should be prepared successfully
      assert_equal 0, status.exitstatus
      # Switch to the agent task branch so discovery can find it
      VCSRepo.new(repo_a).checkout_branch('feat')
      repo_b, remote_b = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo_b, branch: 'feat', lines: ['task b'], push_to_remote: push_flag, tool: ab)
      # second repo should also be prepared successfully
      assert_equal 0, status.exitstatus
      # Switch to the agent task branch so discovery can find it
      VCSRepo.new(repo_b).checkout_branch('feat')
      outer = Dir.mktmpdir('outer')
      FileUtils.mv(repo_a, File.join(outer, 'a'))
      FileUtils.mv(repo_b, File.join(outer, 'b'))
      status2, output = run_get_task(outer, tool: gb)
      # get-task should return tasks for both repositories
      assert_equal 0, status2.exitstatus
      assert_includes output, 'In directory `a`'
      assert_includes output, 'task a'
      assert_includes output, 'In directory `b`'
      assert_includes output, 'task b'
    ensure
      FileUtils.remove_entry(outer) if outer && File.exist?(outer)
      FileUtils.remove_entry(remote_a) if remote_a && File.exist?(remote_a)
      FileUtils.remove_entry(remote_b) if remote_b && File.exist?(remote_b)
    end
  end
end

class GetTaskGitTest < Minitest::Test
  include RepoTestHelper
  include GetTaskCases

  VCS_TYPE = :git
end

class GetTaskHgTest < Minitest::Test
  include RepoTestHelper
  include GetTaskCases

  VCS_TYPE = :hg
end

class GetTaskFossilTest < Minitest::Test
  include RepoTestHelper
  include GetTaskCases

  VCS_TYPE = :fossil
end
