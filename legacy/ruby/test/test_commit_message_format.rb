# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require 'open3'
require_relative 'test_helper'
require_relative '../lib/vcs_repo'
require_relative '../lib/agent_tasks'

# Test class for commit message format and extraction functionality
class TestCommitMessageFormat < Minitest::Test
  include RepoTestHelper

  def test_commit_message_with_https_remote
    repo, remote = setup_repo(:git)

    # Set up HTTPS remote
    vcs_repo = VCSRepo.new(repo)
    git(repo, 'remote', 'set-url', 'origin', 'https://github.com/testuser/test-repo.git')

    status, = run_agent_task(repo, branch: 'feature-branch', lines: ['test task'], push_to_remote: false)
    assert_equal 0, status.exitstatus

    # Verify commit message format
    vcs_repo.checkout_branch('feature-branch')
    first_commit = vcs_repo.first_commit_in_current_branch
    commit_msg = vcs_repo.commit_message(first_commit)

    assert_includes commit_msg, 'Start-Agent-Branch: feature-branch'
    assert_includes commit_msg, 'Target-Remote: https://github.com/testuser/test-repo.git'

    # Verify no empty line between Start-Agent-Branch and Target-Remote
    lines = commit_msg.split("\n")
    start_idx = lines.find_index { |line| line.start_with?('Start-Agent-Branch:') }
    target_idx = lines.find_index { |line| line.start_with?('Target-Remote:') }

    assert start_idx, 'Start-Agent-Branch line not found'
    assert target_idx, 'Target-Remote line not found'
    assert_equal start_idx + 1, target_idx, 'Expected Target-Remote immediately after Start-Agent-Branch'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_commit_message_with_ssh_remote_conversion
    repo, remote = setup_repo(:git)

    # Set up SSH remote
    vcs_repo = VCSRepo.new(repo)
    git(repo, 'remote', 'set-url', 'origin', 'git@github.com:testuser/test-repo.git')

    status, = run_agent_task(repo, branch: 'ssh-test', lines: ['ssh test task'], push_to_remote: false)
    assert_equal 0, status.exitstatus

    # Verify SSH URL is converted to HTTPS in commit message
    vcs_repo.checkout_branch('ssh-test')
    first_commit = vcs_repo.first_commit_in_current_branch
    commit_msg = vcs_repo.commit_message(first_commit)

    assert_includes commit_msg, 'Start-Agent-Branch: ssh-test'
    assert_includes commit_msg, 'Target-Remote: https://github.com/testuser/test-repo.git'
    refute_includes commit_msg, 'git@github.com'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_agent_tasks_extraction_with_remote_and_branch
    repo, remote = setup_repo(:git)

    # Set up HTTPS remote
    vcs_repo = VCSRepo.new(repo)
    git(repo, 'remote', 'set-url', 'origin', 'https://github.com/testuser/test-repo.git')

    # Create task
    status, = run_agent_task(repo, branch: 'extract-test', lines: ['extraction test task'], push_to_remote: false)
    assert_equal 0, status.exitstatus

    # Switch to the task branch and test extraction
    vcs_repo.checkout_branch('extract-test')
    agent_tasks = AgentTasks.new(repo)

    # Autopush should automatically set up the work environment
    message = agent_tasks.agent_prompt_with_autopush_setup
    assert_includes message, 'extraction test task'

    # Verify that work environment was automatically set up
    remotes = capture(repo, 'git', 'remote').strip.split("\n")
    assert_includes remotes, 'target_remote'

    # Verify target_remote points to correct URL
    target_url = capture(repo, 'git', 'remote', 'get-url', 'target_remote').strip
    assert_equal 'https://github.com/testuser/test-repo.git', target_url

    # Verify that making a commit in the local repo would result in the
    # commit appearing in the target repo as well due to the automatically
    # installed post-commit hook
    # TODO (implement this check)
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_agent_tasks_autopush_without_token
    repo, remote = setup_repo(:git)

    # Set up HTTPS remote
    vcs_repo = VCSRepo.new(repo)
    git(repo, 'remote', 'set-url', 'origin', 'https://github.com/testuser/test-repo.git')

    # Create task
    status, = run_agent_task(repo, branch: 'token-test', lines: ['token test task'], push_to_remote: false)
    assert_equal 0, status.exitstatus

    # Switch to the task branch
    vcs_repo.checkout_branch('token-test')
    agent_tasks = AgentTasks.new(repo)

    agent_tasks.agent_prompt_with_autopush_setup

    # Verify that work environment was automatically set up
    remotes = capture(repo, 'git', 'remote').strip.split("\n")
    assert_includes remotes, 'target_remote'

    # Verify target_remote points to correct URL
    target_url = capture(repo, 'git', 'remote', 'get-url', 'target_remote').strip
    assert_equal 'https://github.com/testuser/test-repo.git', target_url
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_agent_tasks_autopush_errors_on_missing_commit_data
    repo, remote = setup_repo(:git)

    # Set up HTTPS remote but don't create a proper task commit
    git(repo, 'remote', 'set-url', 'origin', 'https://github.com/testuser/test-repo.git')

    # Create a regular commit without task metadata
    git(repo, 'checkout', '-b', 'broken-test')
    File.write(File.join(repo, 'test.txt'), 'regular commit')
    git(repo, 'add', 'test.txt')
    git(repo, 'commit', '-m', 'Regular commit without task metadata')

    agent_tasks = AgentTasks.new(repo)

    error = assert_raises(StandardError) do
      agent_tasks.agent_prompt_with_autopush_setup
    end
    assert_includes error.message, 'You are not currently on a agent task branch'
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end
end
