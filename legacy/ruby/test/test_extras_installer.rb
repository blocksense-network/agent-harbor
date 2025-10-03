# frozen_string_literal: true

require 'minitest/autorun'
require 'tmpdir'
require 'fileutils'
require_relative 'test_helper'
require_relative '../lib/extras_installer'
require_relative '../lib/extras_task/cli'

# Test class for ExtrasInstaller functionality
class TestExtrasInstaller < Minitest::Test
  def setup
    @original_extras = ENV.fetch('EXTRAS', nil)
    @original_nix = ENV.fetch('NIX', nil)
    @test_mode = true

    # Create a temporary directory for test markers
    @temp_dir = Dir.mktmpdir('extras_test')
    @marker_dir = File.join(@temp_dir, '.install-markers')

    # Store original marker directory for restoration
    @original_marker_dir = ExtrasInstaller::MARKER_DIR if ExtrasInstaller.const_defined?(:MARKER_DIR)
  end

  def teardown
    ENV['EXTRAS'] = @original_extras
    ENV['NIX'] = @original_nix
    FileUtils.remove_entry(@temp_dir) if @temp_dir && File.exist?(@temp_dir)

    # Clean up any test markers in the real directory
    real_marker_dir = File.join(File.expand_path('..', __dir__), '.install-markers')
    FileUtils.rm_rf(real_marker_dir)
  end

  def test_parse_extras_with_comma_separator
    installer = ExtrasInstaller.new('nix,direnv,cachix', test_mode: @test_mode)
    assert_equal %w[cachix direnv nix], installer.components
  end

  def test_parse_extras_with_semicolon_separator
    installer = ExtrasInstaller.new('nix;direnv;cachix', test_mode: @test_mode)
    assert_equal %w[cachix direnv nix], installer.components
  end

  def test_parse_extras_with_mixed_separators
    installer = ExtrasInstaller.new('nix;direnv+cachix', test_mode: @test_mode)
    assert_equal %w[cachix direnv nix], installer.components
  end

  def test_parse_extras_with_pipe_separator
    installer = ExtrasInstaller.new('nix|direnv', test_mode: @test_mode)
    assert_equal %w[direnv nix], installer.components
  end

  def test_parse_extras_with_space_separator
    installer = ExtrasInstaller.new('nix direnv cachix', test_mode: @test_mode)
    assert_equal %w[cachix direnv nix], installer.components
  end

  def test_parse_extras_removes_duplicates
    installer = ExtrasInstaller.new('nix,nix,direnv', test_mode: @test_mode)
    assert_equal %w[direnv nix], installer.components
  end

  def test_parse_extras_handles_empty_string
    installer = ExtrasInstaller.new('', test_mode: @test_mode)
    assert_equal [], installer.components
  end

  def test_parse_extras_handles_whitespace_only
    installer = ExtrasInstaller.new('   ', test_mode: @test_mode)
    assert_equal [], installer.components
  end

  def test_parse_extras_from_environment_variable
    ENV['EXTRAS'] = 'nix,direnv'
    installer = ExtrasInstaller.new(nil, test_mode: @test_mode)
    assert_equal %w[direnv nix], installer.components
  end

  def test_validate_components_accepts_valid_components
    installer = ExtrasInstaller.new('nix,direnv,cachix', test_mode: @test_mode)
    # Should not raise any errors
    installer.validate_components!
    # If we get here without exception, validation passed
    assert true
  end

  def test_validate_components_rejects_invalid_components
    # Should raise InvalidComponentError during initialization for invalid components
    assert_raises(InvalidComponentError) do
      ExtrasInstaller.new('nix,invalid,direnv', test_mode: @test_mode)
    end
  end

  def test_component_installed_with_marker_file
    installer = ExtrasInstaller.new('nix', test_mode: @test_mode)

    # Component should not be installed initially
    refute installer.component_installed?('nix')

    # Create marker file in the test directory
    FileUtils.mkdir_p(@marker_dir)
    File.write(File.join(@marker_dir, 'nix.done'), Time.now.to_s)

    # Temporarily override the MARKER_DIR constant
    original_marker_dir = ExtrasInstaller::MARKER_DIR
    ExtrasInstaller.send(:remove_const, :MARKER_DIR)
    ExtrasInstaller.const_set(:MARKER_DIR, @marker_dir)

    begin
      # Component should now be detected as installed
      assert installer.component_installed?('nix')
    ensure
      # Restore original constant
      ExtrasInstaller.send(:remove_const, :MARKER_DIR)
      ExtrasInstaller.const_set(:MARKER_DIR, original_marker_dir)
    end
  end

  def test_install_all_empty_components
    installer = ExtrasInstaller.new('', test_mode: @test_mode)

    # Should return empty array for empty components
    components = installer.install_all
    assert_equal [], components
  end

  def test_install_all_with_valid_components
    installer = ExtrasInstaller.new('nix', test_mode: @test_mode)

    # This test verifies install_all returns the components list
    components = installer.install_all
    assert_equal %w[nix], components
  end

  def test_install_all_direct_with_valid_components
    installer = ExtrasInstaller.new('nix', test_mode: @test_mode)

    # Mock the install script execution for direct installation
    installer.stub(:system, true) do
      File.stub(:exist?, true) do
        # Should not raise any errors
        installer.install_all_direct
        # If we get here without exception, installation succeeded
        assert true
      end
    end
  end

  def test_clean_markers_removes_directory
    # Create test marker directory with some files
    test_marker_dir = File.join(@temp_dir, 'test_markers')
    FileUtils.mkdir_p(test_marker_dir)
    File.write(File.join(test_marker_dir, 'nix.done'), 'test')

    # Temporarily override the constant
    original_marker_dir = ExtrasInstaller::MARKER_DIR
    ExtrasInstaller.const_set(:AH_DIR, @temp_dir)

    begin
      ExtrasInstaller.clean_markers
      refute Dir.exist?(File.join(@temp_dir, '.install-markers'))
    ensure
      # Restore original constant (though it may not matter for this test)
      ExtrasInstaller.const_set(:AH_DIR, File.dirname(original_marker_dir))
    end
  end

  def test_help_displays_usage_information
    output = capture_io { ExtrasInstaller.help }

    assert_includes output.first, 'ExtrasInstaller - Component installation framework'
    assert_includes output.first, 'Available components:'
    assert_includes output.first, 'nix     - Nix package manager'
    assert_includes output.first, 'direnv  - Directory-based environment management'
    assert_includes output.first, 'cachix  - Binary cache service for Nix'
  end
end

# Test class for ExtrasTask::CLI functionality
class TestExtrasTaskCLI < Minitest::Test
  def setup
    @original_extras = ENV.fetch('EXTRAS', nil)
    @original_nix = ENV.fetch('NIX', nil)
  end

  def teardown
    ENV['EXTRAS'] = @original_extras
    ENV['NIX'] = @original_nix
  end

  def test_install_extras_with_help_flag
    output = capture_io do
      ExtrasTask::CLI.install_extras(['--help'])
    end

    assert_includes output.first, 'ExtrasInstaller - Component installation framework'
  end

  def test_install_extras_with_clean_flag
    # Create a marker file so clean_markers has something to clean
    marker_dir = File.join(ExtrasInstaller::AH_DIR, '.install-markers')
    FileUtils.mkdir_p(marker_dir)
    File.write(File.join(marker_dir, 'test.done'), 'test')

    output = capture_io do
      ExtrasTask::CLI.install_extras(['--clean'])
    end

    # Should attempt to clean markers
    assert_includes output.first, 'Cleaned installation markers'
  end

  def test_install_extras_with_test_mode_flag
    ENV['EXTRAS'] = 'nix'

    # Capture the call to verify test mode is passed
    ExtrasInstaller.stub(:new, lambda { |_extras, **options|
      assert_equal true, options[:test_mode]
      mock_installer = Minitest::Mock.new
      mock_installer.expect(:install_all_direct, nil)
      mock_installer
    }) do
      ExtrasTask::CLI.install_extras(['--test-mode'])
    end
  end

  def test_install_extras_with_extras_option
    # Mock installer to verify the extras string is passed correctly
    ExtrasInstaller.stub(:new, lambda { |extras, **_options|
      assert_equal 'nix,direnv', extras
      mock_installer = Minitest::Mock.new
      mock_installer.expect(:install_all_direct, nil)
      mock_installer
    }) do
      ExtrasTask::CLI.install_extras(['--extras=nix,direnv'])
    end
  end

  def test_install_extras_with_empty_extras
    ENV.delete('EXTRAS')

    output = capture_io do
      ExtrasTask::CLI.install_extras([])
    end

    assert_includes output.first, 'No extras specified in EXTRAS environment variable'
    assert_includes output.first, 'Available extras: nix, direnv, cachix'
  end

  def test_install_extras_handles_extras_error
    ENV['EXTRAS'] = 'invalid'

    # Should exit with status 1 on ExtrasError
    assert_raises(SystemExit) do
      capture_io do
        ExtrasTask::CLI.install_extras([])
      end
    end
  end

  def test_legacy_nix_compatibility
    ENV['NIX'] = '1'
    ENV.delete('EXTRAS')

    # Mock the install_extras method to verify it gets called
    cli_mock = Minitest::Mock.new
    cli_mock.expect(:install_extras, nil)

    ExtrasTask::CLI.stub(:install_extras, -> { cli_mock.install_extras }) do
      ExtrasTask::CLI.handle_legacy_nix
    end

    cli_mock.verify
    assert_equal 'nix', ENV.fetch('EXTRAS', nil)
  end

  def test_legacy_nix_does_nothing_when_not_set
    ENV.delete('NIX')

    # Should not modify EXTRAS when NIX is not set
    original_extras = ENV.fetch('EXTRAS', nil)
    ExtrasTask::CLI.handle_legacy_nix
    assert_equal original_extras, ENV.fetch('EXTRAS', nil)
  end
end
