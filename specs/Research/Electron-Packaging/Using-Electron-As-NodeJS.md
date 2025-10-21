## Can I use the node.js bundled with Electron for running command-line node.js programs?

Short answer: kinda—Electron does bundle a Node.js runtime, and you _can_ run scripts with it, but it’s not meant to replace a normal `node` binary for CLI work. Electron embeds a specific Node/V8 inside the Electron executable, not as a standalone `node` program. ([Electron][1])

# If you still want to do it

Electron has a special “run as Node” mode:

**macOS/Linux**

```bash
ELECTRON_RUN_AS_NODE=1 ./node_modules/.bin/electron path/to/script.js --your-args
```

**Windows (PowerShell)**

```powershell
$env:ELECTRON_RUN_AS_NODE=1; .\node_modules\.bin\electron.cmd .\path\to\script.js --your-args
```

That environment variable tells Electron to act like Node for that process. In this mode most Node CLI flags work, except a few crypto/OpenSSL-related ones (Electron builds Node against BoringSSL). Also, app authors can disable this mode entirely using the `runAsNode` fuse—if it’s disabled, the env var is ignored. ([Electron][2])

# Why it’s usually a bad idea for general CLI use

- **Pinned/odd versioning:** Electron’s Node version is tied to its Chromium/V8 version, which may lag or jump independently of the official Node release line. ([Electron][1])
- **Native module ABI mismatch:** Any native addons must be rebuilt for Electron’s ABI; binaries compiled for regular Node won’t load. ([GitHub][3])
- **Feature differences:** Some Node flags are disabled under Electron’s crypto stack (see above). ([Electron][2])
- **May be disabled:** Packaged apps can (and increasingly do) ship with `runAsNode` turned off for security. ([Electron][4])

# Recommended instead

Install a standard Node.js runtime (e.g., via nvm/asdf) for CLI tools, and reserve Electron for desktop app processes. If you need to ship a CLI alongside your Electron app, consider publishing a separate CLI (real Node), or bundling one with tools like `pkg`/`nexe`—avoids the ABI and fuse pitfalls.

If you tell me your OS and what CLI you want to run, I can show the exact setup.

[1]: https://www.electronjs.org/docs/latest?utm_source=chatgpt.com 'Introduction | Electron'
[2]: https://www.electronjs.org/docs/latest/api/environment-variables?utm_source=chatgpt.com 'Environment Variables - Electron'
[3]: https://github.com/electron/electron/blob/main/docs/tutorial/using-native-node-modules.md?utm_source=chatgpt.com 'electron/docs/tutorial/using-native-node-modules.md at main - GitHub'

[4]: https://www.electronjs.org/blog/statement-run-as-node-cves?utm_source=chatgpt.com "Statement regarding \"runAsNode\" CVEs - Electron"
