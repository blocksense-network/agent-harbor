# frozen_string_literal: true

require_relative 'test_helper'
require 'snapshot/provider'
require 'mock_agent'
require 'tmpdir'
require 'benchmark'
require 'etc'

# Performance and scalability tests for MockAgent and snapshot providers
# Measures resource usage, timing, and system limits under various loads
class TestMockAgentPerformance < Minitest::Test
  private

  def process_memory
    # Get memory usage in KB using ps command
    output = `ps -o rss= -p #{Process.pid}`.strip
    output.to_i
  rescue StandardError
    0 # Return 0 if memory monitoring fails
  end

  def setup
    @original_repo = Dir.mktmpdir('perf_test_repo')
    @workspaces = []
    @provider = Snapshot.provider_for(@original_repo)

    create_test_repository

    puts "\n#{'=' * 60}"
    puts "Performance Testing with #{@provider.class.name}"
    puts '=' * 60
  end

  def teardown
    cleanup_workspaces
    FileUtils.rm_rf(@original_repo)
  end

  def test_snapshot_creation_performance
    puts "\n--- Snapshot Creation Performance ---"

    num_snapshots = 10
    creation_times = []

    # Warm up
    warm_workspace = create_workspace('warmup')
    cleanup_workspace(warm_workspace)

    # Measure snapshot creation times
    Benchmark.bm(20) do |x|
      x.report('Sequential creation:') do
        (1..num_snapshots).each do |i|
          start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          create_workspace("perf_seq_#{i}")
          end_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          creation_times << (end_time - start_time)
        end
      end
    end

    # Analyze results
    avg_time = creation_times.sum / creation_times.size
    min_time = creation_times.min
    max_time = creation_times.max
    std_dev = Math.sqrt(creation_times.map { |t| (t - avg_time)**2 }.sum / creation_times.size)

    puts "\nSnapshot Creation Statistics:"
    puts "  Snapshots created: #{num_snapshots}"
    puts "  Average time: #{avg_time.round(3)}s"
    puts "  Min time: #{min_time.round(3)}s"
    puts "  Max time: #{max_time.round(3)}s"
    puts "  Std deviation: #{std_dev.round(3)}s"
    puts "  Provider: #{@provider.class.name}"

    # Performance expectations based on provider type
    case @provider
    when Snapshot::ZfsProvider, Snapshot::BtrfsProvider
      assert avg_time < 1.0, "CoW snapshots should average < 1s (actual: #{avg_time.round(3)}s)"
      assert max_time < 3.0, "CoW snapshots should max < 3s (actual: #{max_time.round(3)}s)"
    when Snapshot::CopyProvider
      assert avg_time < 10.0, "Copy provider should average < 10s (actual: #{avg_time.round(3)}s)"
    end
  end

  def test_concurrent_snapshot_creation
    puts "\n--- Concurrent Snapshot Creation ---"

    # Choose concurrency level based on available processors
    max_concurrent = [Etc.nprocessors * 2, 8].min
    num_concurrent = [2, 4, 8].select { |n| n <= max_concurrent }.max || 2

    num_concurrent.times do |concurrency|
      next if concurrency.zero?

      puts "\nTesting #{concurrency + 1} concurrent snapshots:"

      start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

      threads = (1..(concurrency + 1)).map do |i|
        Thread.new do
          Thread.current[:start] = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          workspace = create_workspace("concurrent_#{concurrency}_#{i}")
          Thread.current[:end] = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          Thread.current[:workspace] = workspace
        end
      end

      threads.each(&:join)
      total_time = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time

      individual_times = threads.map { |t| t[:end] - t[:start] }
      avg_individual = individual_times.sum / individual_times.size

      puts "  Total time: #{total_time.round(3)}s"
      puts "  Average individual: #{avg_individual.round(3)}s"
      puts "  Efficiency: #{((avg_individual / total_time) * 100).round(1)}%"

      # Clean up these workspaces immediately to avoid resource issues
      threads.each do |t|
        cleanup_workspace(t[:workspace])
      end
    end
  end

  def test_agent_execution_performance
    puts "\n--- Agent Execution Performance ---"

    workspace = create_workspace('agent_perf')

    # Test different work durations
    durations = [0.5, 1.0, 2.0, 5.0]

    durations.each do |target_duration|
      puts "\nTesting #{target_duration}s target duration:"

      agent = MockAgent.new(workspace, "perf_#{target_duration}", {
                              min_work_duration: target_duration * 0.9,
                              max_work_duration: target_duration * 1.1,
                              create_file_probability: 0.5,
                              modify_file_probability: 0.3,
                              sleep_between_ops: 0.01
                            })

      start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      result = agent.run_work_session
      actual_duration = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time

      assert result[:success], 'Agent should complete successfully'

      puts "  Target: #{target_duration}s, Actual: #{actual_duration.round(3)}s"
      puts "  Activities: #{result[:activity_count]}"
      puts "  Overhead: #{((actual_duration - result[:duration]) * 1000).round(1)}ms"

      # Verify timing is reasonable
      assert actual_duration >= target_duration * 0.8, 'Should take at least 80% of target time'
      assert actual_duration <= target_duration * 2.0, 'Should not take more than 200% of target time'
    end
  end

  def test_memory_usage_under_load
    puts "\n--- Memory Usage Under Load ---"

    # Only run this test on systems with memory monitoring
    if system('which ps > /dev/null 2>&1')
      initial_memory = process_memory

      num_agents = 6
      workspaces = (1..num_agents).map { |i| create_workspace("memory_#{i}") }

      # Run agents concurrently
      threads = workspaces.map.with_index do |workspace, i|
        Thread.new do
          agent = MockAgent.new(workspace, "memory_#{i + 1}", {
                                  min_work_duration: 1.0,
                                  max_work_duration: 2.0,
                                  create_file_probability: 0.8,
                                  max_generated_lines: 200
                                })
          agent.run_work_session
        end
      end

      # Monitor memory during execution
      max_memory = initial_memory
      monitor_thread = Thread.new do
        while threads.any?(&:alive?)
          current_memory = process_memory
          max_memory = [max_memory, current_memory].max
          sleep(0.1)
        end
      end

      threads.each(&:join)
      monitor_thread.kill

      final_memory = process_memory
      memory_increase = max_memory - initial_memory

      puts "  Initial memory: #{initial_memory} KB"
      puts "  Peak memory: #{max_memory} KB"
      puts "  Final memory: #{final_memory} KB"
      puts "  Peak increase: #{memory_increase} KB"
      puts "  Per agent avg: #{(memory_increase / num_agents).round(1)} KB"

      # Memory usage should be reasonable
      assert memory_increase < 100_000, "Memory increase should be < 100MB (actual: #{memory_increase} KB)"
    else
      skip 'Memory monitoring not available on this system'
    end
  end

  def test_file_system_scalability
    puts "\n--- File System Scalability ---"

    # Test with repositories of different sizes
    repo_sizes = [
      { name: 'small', files: 10, size_kb: 10 },
      { name: 'medium', files: 100, size_kb: 100 },
      { name: 'large', files: 500, size_kb: 1000 }
    ]

    repo_sizes.each do |repo_config|
      puts "\nTesting #{repo_config[:name]} repository (#{repo_config[:files]} files, ~#{repo_config[:size_kb]}KB):"

      # Create test repo of specified size
      test_repo = Dir.mktmpdir("scale_test_#{repo_config[:name]}")

      begin
        create_sized_repository(test_repo, repo_config[:files], repo_config[:size_kb])
        provider = Snapshot.provider_for(test_repo)

        # Measure snapshot creation
        start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
        workspace = Dir.mktmpdir("workspace_#{repo_config[:name]}")
        provider.create_workspace(workspace)
        creation_time = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time

        # Measure agent execution
        agent = MockAgent.new(workspace, "scale_#{repo_config[:name]}", {
                                min_work_duration: 0.5,
                                max_work_duration: 1.0
                              })

        start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)
        result = agent.run_work_session
        agent_time = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time

        puts "  Snapshot creation: #{creation_time.round(3)}s"
        puts "  Agent execution: #{agent_time.round(3)}s"
        puts "  Total activities: #{result[:activity_count]}"

        # Cleanup
        provider.cleanup_workspace(workspace) if provider.respond_to?(:cleanup_workspace)
        FileUtils.rm_rf(workspace)

        # Performance should scale reasonably
        assert creation_time < repo_config[:files] * 0.1, 'Creation time should scale reasonably with repo size'
      ensure
        FileUtils.rm_rf(test_repo)
      end
    end
  end

  def test_stress_test_many_concurrent_agents
    puts "\n--- Stress Test: Many Concurrent Agents ---"

    # Scale based on system capabilities
    max_agents = [Etc.nprocessors * 3, 16].min
    puts "Testing up to #{max_agents} concurrent agents"

    [2, 4, max_agents].uniq.each do |num_agents|
      next if num_agents > max_agents

      puts "\nStress testing #{num_agents} concurrent agents:"

      # Create workspaces
      start_setup = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      workspaces = (1..num_agents).map { |i| create_workspace("stress_#{i}") }
      setup_time = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_setup

      # Launch agents
      start_execution = Process.clock_gettime(Process::CLOCK_MONOTONIC)
      threads = workspaces.map.with_index do |workspace, i|
        Thread.new do
          agent = MockAgent.new(workspace, "stress_#{i + 1}", {
                                  min_work_duration: 0.5,
                                  max_work_duration: 1.5,
                                  create_file_probability: 0.6,
                                  modify_file_probability: 0.4,
                                  sleep_between_ops: 0.01
                                })

          start = Process.clock_gettime(Process::CLOCK_MONOTONIC)
          result = agent.run_work_session
          duration = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start

          {
            success: result[:success],
            duration: duration,
            activities: result[:activity_count]
          }
        end
      end

      results = threads.map(&:join).map(&:value)
      total_execution = Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_execution

      # Analyze results
      successful = results.count { |r| r[:success] }
      avg_duration = results.map { |r| r[:duration] }.sum / results.size
      total_activities = results.map { |r| r[:activities] }.sum

      puts "  Setup time: #{setup_time.round(3)}s"
      puts "  Execution time: #{total_execution.round(3)}s"
      puts "  Success rate: #{successful}/#{num_agents} (#{(successful.to_f / num_agents * 100).round(1)}%)"
      puts "  Average agent time: #{avg_duration.round(3)}s"
      puts "  Total activities: #{total_activities}"
      puts "  Throughput: #{(total_activities / total_execution).round(1)} activities/sec"

      # Performance expectations
      assert successful == num_agents, 'All agents should complete successfully'
      assert setup_time < num_agents * 5.0, 'Setup should be reasonably fast'
      assert total_execution < 10.0, 'Execution should complete within reasonable time'

      # Immediate cleanup for stress test
      workspaces.each { |ws| cleanup_workspace(ws) }
      @workspaces.clear
    end
  end

  def create_test_repository
    # Create a basic repository with some test files
    FileUtils.mkdir_p(File.join(@original_repo, 'src'))
    FileUtils.mkdir_p(File.join(@original_repo, 'test'))

    # Add some files for realistic testing
    File.write(File.join(@original_repo, 'README.md'),
               "# Performance Test Repository\n\nThis repository is used for performance testing.\n")
    File.write(File.join(@original_repo, 'src', 'main.rb'), "puts 'Hello, World!'\n")
    File.write(File.join(@original_repo, 'src', 'utils.rb'),
               "module Utils\n  def self.helper\n    'helper'\n  end\nend\n")
    File.write(File.join(@original_repo, 'test', 'test_main.rb'),
               "require 'minitest/autorun'\n\nclass TestMain < Minitest::Test\nend\n")
  end

  def create_sized_repository(path, num_files, total_size_kb)
    FileUtils.mkdir_p(File.join(path, 'src'))
    FileUtils.mkdir_p(File.join(path, 'test'))
    FileUtils.mkdir_p(File.join(path, 'docs'))

    # Calculate approximate size per file
    size_per_file = (total_size_kb * 1024) / num_files

    (1..num_files).each do |i|
      subdir = case i % 3
               when 0 then 'src'
               when 1 then 'test'
               else 'docs'
               end

      file_path = File.join(path, subdir, "file_#{i}.txt")
      content = "File #{i}\n" + ('x' * [size_per_file - 10, 10].max)
      File.write(file_path, content)
    end
  end

  def create_workspace(name)
    workspace = Dir.mktmpdir("perf_workspace_#{name}")
    @workspaces << workspace
    @provider.create_workspace(workspace)
    workspace
  end

  def cleanup_workspace(workspace)
    return unless workspace && Dir.exist?(workspace)

    begin
      @provider.cleanup_workspace(workspace) if @provider.respond_to?(:cleanup_workspace)
      FileUtils.rm_rf(workspace)
    rescue StandardError => e
      puts "Warning: Failed to cleanup #{workspace}: #{e.message}"
    end
  end

  def cleanup_workspaces
    @workspaces.each { |ws| cleanup_workspace(ws) }
    @workspaces.clear
  end
end
