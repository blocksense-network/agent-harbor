# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative 'test_helper'

module FollowUpCases
  def test_append_follow_up_tasks
    RepoTestHelper::AGENT_TASK_BINARIES.product(RepoTestHelper::GET_TASK_BINARIES).each do |ab, gb|
      repo, remote = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'feat', lines: ['task one'], push_to_remote: push_flag, tool: ab)
      assert_equal 0, status.exitstatus, 'initial task failed'
      r = VCSRepo.new(repo)
      r.checkout_branch('feat')
      File.write(File.join(repo, 'work.txt'), 'work')
      r.commit_file(File.join(repo, 'work.txt'), 'work commit')
      push_flag = self.class::VCS_TYPE != :fossil
      status2, = run_agent_task(repo, branch: nil, lines: ['task two'], push_to_remote: push_flag, tool: ab)
      assert_equal 0, status2.exitstatus, 'follow-up task failed'
      status3, output = run_get_task(repo, tool: gb)
      assert_equal 0, status3.exitstatus, 'get-task failed after follow-up'
      assert_includes output, 'task one', 'original task missing'
      assert_includes output, 'task two', 'follow-up task missing'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_refuse_on_main_branch
    RepoTestHelper::AGENT_TASK_BINARIES.each do |ab|
      repo, remote = setup_repo(self.class::VCS_TYPE)
      status, = run_agent_task(repo, branch: nil, lines: ['bad'], push_to_remote: false, tool: ab)
      assert status.exitstatus != 0, 'expected failure on main branch'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end

  def test_nested_branch_tasks
    # Use only direct binaries, skip gem-based ones to avoid installation issues
    [[RepoTestHelper::AGENT_TASK, RepoTestHelper::GET_TASK]].each do |ab, gb|
      repo, remote = setup_repo(self.class::VCS_TYPE)
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'a', lines: ['task a'], push_to_remote: push_flag, tool: ab)
      assert_equal 0, status.exitstatus, 'branch a failed'
      r = VCSRepo.new(repo)
      r.checkout_branch('a')
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'b', lines: ['task b'], push_to_remote: push_flag, tool: ab)
      assert_equal 0, status.exitstatus, 'branch b failed'
      r.checkout_branch('b')
      _, output = run_get_task(repo, tool: gb)
      assert_includes output, 'task b', 'task b missing'
      refute_includes output, 'task a', 'task a should not appear in branch b'
      push_flag = self.class::VCS_TYPE != :fossil
      status, = run_agent_task(repo, branch: 'c', lines: ['task c'], push_to_remote: push_flag, tool: ab)
      assert_equal 0, status.exitstatus, 'branch c failed'
      r.checkout_branch('c')
      _, output = run_get_task(repo, tool: gb)
      assert_includes output, 'task c', 'task c missing'
      refute_includes output, 'task b', 'task b should not appear in branch c'
    ensure
      FileUtils.remove_entry(repo) if repo && File.exist?(repo)
      FileUtils.remove_entry(remote) if remote && File.exist?(remote)
    end
  end
end

class FollowUpGitTest < Minitest::Test
  include RepoTestHelper
  include FollowUpCases

  VCS_TYPE = :git
end

class FollowUpHgTest < Minitest::Test
  include RepoTestHelper
  include FollowUpCases

  VCS_TYPE = :hg
end

class FollowUpFossilTest < Minitest::Test
  include RepoTestHelper
  include FollowUpCases

  VCS_TYPE = :fossil
end
