Yes, here's a sketched solution using DYLD_INSERT_LIBRARIES to inject a custom dylib that interposes both bind() and connect() syscalls. This allows runtime remapping of IPs (e.g., redirecting binds/connections from 127.0.0.1 to 127.0.0.2) or ports (e.g., shifting a specific port to another) without modifying the original executable. The interposition happens transparently when the process calls these functions.

This assumes:

- The target processes are not hardened against dyld injection (i.e., they lack the hardened runtime or have the `com.apple.security.cs.allow-dyld-environment-variables` entitlement).
- You've configured loopback aliases (e.g., `sudo ifconfig lo0 alias 127.0.0.2 up`).
- For simplicity, the sketch focuses on IPv4 (AF_INET); extend to IPv6 (AF_INET6) by checking `sin6_family` and modifying `sin6_addr` similarly.
- Remapping is hardcoded here (e.g., IP from "127.0.0.1" to "127.0.0.2", or port 8080 to 8081), but you could make it configurable via environment variables read in the dylib.

### Step-by-Step Solution Sketch

1. **Create the Interposing Dylib**:
   - Write a C file (e.g., `remap.c`) that defines replacement functions for bind() and connect().
   - Use the `DYLD_INTERPOSE` macro from `<mach-o/dyld-interposing.h>` to hook the originals.
   - In the replacements:
     - Cast the `sockaddr` to `sockaddr_in` (for IPv4).
     - Inspect and modify the IP (via `sin_addr.s_addr`) or port (via `sin_port`).
     - Call the original function with the updated struct.
   - Compile: `clang -dynamiclib -o remap.dylib remap.c`.
   - Sign: `codesign -f -s - remap.dylib` (ad-hoc signing is fine for local use).

   Here's the code sketch for `remap.c`:

   ```
   #include <sys/types.h>
   #include <sys/socket.h>
   #include <netinet/in.h>
   #include <arpa/inet.h>
   #include <mach-o/dyld-interposing.h>
   #include <stdio.h>  // For logging, optional

   // Original functions (declared for interpose)
   extern int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen);
   extern int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen);

   // Replacement for bind()
   int my_bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
       if (addr->sa_family == AF_INET && addrlen >= sizeof(struct sockaddr_in)) {
           struct sockaddr_in modified = *(struct sockaddr_in *)addr;  // Copy to modify safely

           // IP remapping example: 127.0.0.1 -> 127.0.0.2
           in_addr_t original_ip = modified.sin_addr.s_addr;
           if (original_ip == inet_addr("127.0.0.1")) {
               modified.sin_addr.s_addr = inet_addr("127.0.0.2");
               fprintf(stderr, "[REMAP] bind IP changed from 127.0.0.1 to 127.0.0.2\n");  // Optional log
           }

           // Port remapping example: 8080 -> 8081
           uint16_t original_port = ntohs(modified.sin_port);
           if (original_port == 8080) {
               modified.sin_port = htons(8081);
               fprintf(stderr, "[REMAP] bind port changed from 8080 to 8081\n");  // Optional log
           }

           // Call original with modified addr
           return bind(sockfd, (struct sockaddr *)&modified, sizeof(modified));
       }
       // Fallback to original if not IPv4 or too small
       return bind(sockfd, addr, addrlen);
   }

   // Replacement for connect()
   int my_connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
       if (addr->sa_family == AF_INET && addrlen >= sizeof(struct sockaddr_in)) {
           struct sockaddr_in modified = *(struct sockaddr_in *)addr;  // Copy to modify safely

           // IP remapping example: 127.0.0.1 -> 127.0.0.2
           in_addr_t original_ip = modified.sin_addr.s_addr;
           if (original_ip == inet_addr("127.0.0.1")) {
               modified.sin_addr.s_addr = inet_addr("127.0.0.2");
               fprintf(stderr, "[REMAP] connect IP changed from 127.0.0.1 to 127.0.0.2\n");  // Optional log
           }

           // Port remapping example: 8080 -> 8081
           uint16_t original_port = ntohs(modified.sin_port);
           if (original_port == 8080) {
               modified.sin_port = htons(8081);
               fprintf(stderr, "[REMAP] connect port changed from 8080 to 8081\n");  // Optional log
           }

           // Call original with modified addr
           return connect(sockfd, (struct sockaddr *)&modified, sizeof(modified));
       }
       // Fallback to original if not IPv4 or too small
       return connect(sockfd, addr, addrlen);
   }

   // Interpose the functions
   DYLD_INTERPOSE(my_bind, bind);
   DYLD_INTERPOSE(my_connect, connect);
   ```

2. **Launch Processes with Injection**:
   - Use a wrapper script or set the environment variable directly.
   - Example shell command: `DYLD_INSERT_LIBRARIES=/path/to/remap.dylib /path/to/your_process arg1 arg2`
   - For a group of processes, create a launch script:
     ```
     #!/bin/bash
     export DYLD_INSERT_LIBRARIES=/path/to/remap.dylib
     exec "$@"
     ```
     Then run: `./wrapper.sh /path/to/your_process`
   - For daemons/services, modify the launchd plist to include the environment variable under `<key>EnvironmentVariables</key>`.

3. **Testing and Edge Cases**:
   - Test with a simple server/client app that binds/connects to 127.0.0.1:8080; it should remap automatically.
   - Handle errors: If remapping fails (e.g., invalid addrlen), fall back to original.
   - For IPv6: Add checks for `AF_INET6`, cast to `struct sockaddr_in6`, and use `inet_pton`/`inet_ntop` for `sin6_addr`.
   - Logging: The `fprintf` calls help debug; remove for production.
   - Limitations: Won't work on processes with hardened runtime without re-signing. For those, consider alternatives like a proxy wrapper (e.g., launch via `socat` or a custom launcher that remaps).

This provides transparent remapping at runtime. Adjust the conditions in `my_bind` and `my_connect` for your specific port/IP rules.
