#!/usr/bin/env ruby
# frozen_string_literal: true

require 'socket'
require 'json'

SOCKET_PATH = '/tmp/agent-harbor/ah-fs-snapshots-daemon'

def check_daemon_liveness(socket_path)
  return false unless File.socket?(socket_path)

  begin
    UNIXSocket.open(socket_path) do |socket|
      # Send ping command like the tests do
      socket.puts({ 'command' => 'ping' }.to_json)
      response = socket.gets

      if response
        resp = JSON.parse(response.strip)
        # Check if we got a successful response
        return resp['success'] == true || resp.key?('result')
      end
    end
  rescue StandardError
    # Connection failed or invalid response
    return false
  end

  false
end

if check_daemon_liveness(SOCKET_PATH)
  puts "AH filesystem snapshots daemon is running (socket: #{SOCKET_PATH})"
elsif File.exist?(SOCKET_PATH)
  puts 'Warning: Socket exists but daemon is not responding'
else
  puts 'AH filesystem snapshots daemon is not running'
  puts 'Start it with: just legacy-start-ah-fs-snapshots-daemon'
end
