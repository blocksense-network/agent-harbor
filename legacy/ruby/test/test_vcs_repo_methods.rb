# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative 'test_helper'
require_relative '../lib/vcs_repo'

# Test class for VCSRepo functionality
class TestVCSRepoMethods < Minitest::Test
  include RepoTestHelper

  def test_default_remote_http_url_with_https
    repo, remote = setup_repo(:git)
    vcs_repo = VCSRepo.new(repo)

    git(repo, 'remote', 'set-url', 'origin', 'https://github.com/user/repo.git')
    assert_equal 'https://github.com/user/repo.git', vcs_repo.default_remote_http_url
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_default_remote_http_url_with_ssh_conversion
    repo, remote = setup_repo(:git)
    vcs_repo = VCSRepo.new(repo)

    git(repo, 'remote', 'set-url', 'origin', 'git@github.com:user/repo.git')
    assert_equal 'https://github.com/user/repo.git', vcs_repo.default_remote_http_url

    # Test SSH with explicit protocol
    git(repo, 'remote', 'set-url', 'origin', 'ssh://git@gitlab.com/user/repo.git')
    assert_equal 'https://gitlab.com/user/repo.git', vcs_repo.default_remote_http_url
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_default_remote_http_url_no_remote
    repo, remote = setup_repo(:git)
    vcs_repo = VCSRepo.new(repo)

    git(repo, 'remote', 'remove', 'origin')
    assert_nil vcs_repo.default_remote_http_url
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_commit_message_retrieval
    repo, remote = setup_repo(:git)
    vcs_repo = VCSRepo.new(repo)

    # Create a commit with a specific message
    test_message = <<~MSG.chomp
      Test commit message
      With multiple lines
    MSG
    test_file = File.join(repo, 'test.txt')
    File.write(test_file, 'test content')

    git(repo, 'add', 'test.txt')
    git(repo, 'commit', '-m', test_message)
    # Get the latest commit hash
    commit_hash = capture(repo, 'git', 'rev-parse', 'HEAD')
    retrieved_message = vcs_repo.commit_message(commit_hash)
    assert_equal test_message, retrieved_message
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end

  def test_ssh_url_variations
    repo, remote = setup_repo(:git)
    vcs_repo = VCSRepo.new(repo)

    # Test various SSH URL formats
    ssh_urls = [
      'git@github.com:user/repo.git',
      'git@gitlab.com:group/subgroup/repo.git',
      'git@bitbucket.org:user/repo.git',
      'ssh://git@github.com/user/repo.git',
      'ssh://git@gitlab.com:2222/user/repo.git', # Custom port
      'git@custom-host.com:user/repo.git'
    ]

    expected_https = [
      'https://github.com/user/repo.git',
      'https://gitlab.com/group/subgroup/repo.git',
      'https://bitbucket.org/user/repo.git',
      'https://github.com/user/repo.git',
      'https://gitlab.com/user/repo.git', # Port removed
      'https://custom-host.com/user/repo.git'
    ]

    ssh_urls.each_with_index do |ssh_url, index|
      git(repo, 'remote', 'set-url', 'origin', ssh_url)
      result = vcs_repo.default_remote_http_url
      assert_equal expected_https[index], result, "Failed for SSH URL: #{ssh_url}"
    end
  ensure
    FileUtils.remove_entry(repo) if repo && File.exist?(repo)
    FileUtils.remove_entry(remote) if remote && File.exist?(remote)
  end
end
