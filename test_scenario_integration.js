#!/usr/bin/env node

// Simple integration test for scenario functionality
const { spawn } = require('child_process');
const http = require('http');

console.log('🧪 Testing scenario integration...');

const serverProcess = spawn('node', ['./webui/mock-server/dist/index.js', '--scenario=test_scenarios/test_scenario.yaml', '--merge-completed'], {
  stdio: ['pipe', 'pipe', 'pipe'],
  cwd: process.cwd()
});

let serverOutput = '';
let serverReady = false;

serverProcess.stdout.on('data', (data) => {
  const output = data.toString();
  serverOutput += output;
  console.log('SERVER:', output.trim());

  if (output.includes('Mock API server running on http://localhost:3001') && !serverReady) {
    serverReady = true;
    console.log('✅ Server is ready, testing API...');

    // Wait a bit for scenario to complete
    setTimeout(() => {
      testAPI();
    }, 3000);
  }
});

serverProcess.stderr.on('data', (data) => {
  console.error('SERVER ERROR:', data.toString());
});

function testAPI() {
  console.log('📡 Testing sessions API...');

  const req = http.get('http://localhost:3001/api/v1/sessions', (res) => {
    let data = '';

    res.on('data', (chunk) => {
      data += chunk;
    });

    res.on('end', () => {
      try {
        const sessions = JSON.parse(data);
        console.log(`📊 Found ${sessions.items.length} total sessions`);

        const scenarioSessions = sessions.items.filter(s => s.metadata?.scenario);
        console.log(`🎭 Found ${scenarioSessions.length} scenario sessions`);

        if (scenarioSessions.length > 0) {
          console.log('✅ SUCCESS: Scenario sessions found!');
          scenarioSessions.forEach(session => {
            console.log(`   - ${session.id}: ${session.status} (${session.prompt})`);
          });
        } else {
          console.log('❌ FAILED: No scenario sessions found');
        }

        // Test individual session endpoint
        if (scenarioSessions.length > 0) {
          const sessionId = scenarioSessions[0].id;
          console.log(`🔍 Testing individual session ${sessionId}...`);

          const sessionReq = http.get(`http://localhost:3001/api/v1/sessions/${sessionId}`, (sessionRes) => {
            let sessionData = '';
            sessionRes.on('data', (chunk) => sessionData += chunk);
            sessionRes.on('end', () => {
              try {
                const session = JSON.parse(sessionData);
                console.log(`📋 Session status: ${session.status}`);
                console.log(`📝 Session metadata:`, session.metadata);

                cleanup();
              } catch (e) {
                console.error('❌ Failed to parse session response:', e.message);
                cleanup();
              }
            });
          });

          sessionReq.on('error', (e) => {
            console.error('❌ Session request failed:', e.message);
            cleanup();
          });
        } else {
          cleanup();
        }

      } catch (e) {
        console.error('❌ Failed to parse sessions response:', e.message);
        cleanup();
      }
    });
  });

  req.on('error', (e) => {
    console.error('❌ API request failed:', e.message);
    cleanup();
  });

  req.setTimeout(5000, () => {
    console.error('❌ API request timeout');
    cleanup();
  });
}

function cleanup() {
  console.log('🧹 Cleaning up...');
  serverProcess.kill('SIGTERM');

  setTimeout(() => {
    console.log('🏁 Test completed');
    process.exit(0);
  }, 1000);
}

// Timeout after 15 seconds
setTimeout(() => {
  console.error('⏰ Test timeout - server did not start properly');
  cleanup();
}, 15000);

// Handle Ctrl+C
process.on('SIGINT', cleanup);
