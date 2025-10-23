/*
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <arpa/inet.h>
#include <errno.h>
#include <dlfcn.h>

// Environment variable names
#define ENV_STRATEGY "NETWORK_STRATEGY"
#define ENV_LISTENING_BASE_PORT "LISTENING_BASE_PORT"
#define ENV_LISTENING_PORT_COUNT "LISTENING_PORT_COUNT"
#define ENV_LISTENING_LOOPBACK_DEVICE "LISTENING_LOOPBACK_DEVICE"
#define ENV_CONNECT_LOOPBACK_DEVICE "CONNECT_LOOPBACK_DEVICE"

// Strategy types
#define STRATEGY_FAIL "fail"
#define STRATEGY_REWRITE_DEVICE "rewrite_device"
#define STRATEGY_REWRITE_PORT "rewrite_port"

// Original function pointers
static int (*original_bind)(int, const struct sockaddr *, socklen_t) = NULL;
static int (*original_connect)(int, const struct sockaddr *, socklen_t) = NULL;

// Global configuration
static char *strategy = NULL;
static char *listening_loopback_device = NULL;
static char *connect_loopback_device = NULL;
static int listening_base_port = -1;
static int listening_port_count = -1;

// Shared memory for port mapping (Strategy C)
#define PORT_MAP_SIZE 65536
static uint16_t port_map[PORT_MAP_SIZE];

// Helper function to check if address is localhost
static int is_localhost(const struct sockaddr *addr, socklen_t addrlen) {
    if (addr->sa_family == AF_INET) {
        const struct sockaddr_in *addr_in = (const struct sockaddr_in *)addr;
        uint32_t ip = ntohl(addr_in->sin_addr.s_addr);
        return (ip == INADDR_LOOPBACK) || (ip >= 0x7F000001 && ip <= 0x7F0000FF);
    } else if (addr->sa_family == AF_INET6) {
        const struct sockaddr_in6 *addr_in6 = (const struct sockaddr_in6 *)addr;
        return memcmp(&addr_in6->sin6_addr, &in6addr_loopback, sizeof(struct in6_addr)) == 0;
    }
    return 0;
}

// Helper function to check if port is in allowed range (Strategy A)
static int is_port_allowed(uint16_t port) {
    if (listening_base_port < 0 || listening_port_count < 0) {
        return 1; // No restrictions if not configured
    }
    return (port >= listening_base_port && port < listening_base_port + listening_port_count);
}

// Helper function to rewrite address to alternative loopback device
static void rewrite_to_device(struct sockaddr *addr, socklen_t addrlen, const char *device_ip) {
    if (addr->sa_family == AF_INET) {
        struct sockaddr_in *addr_in = (struct sockaddr_in *)addr;
        char original_ip[INET_ADDRSTRLEN];
        inet_ntop(AF_INET, &addr_in->sin_addr, original_ip, sizeof(original_ip));

        if (inet_pton(AF_INET, device_ip, &addr_in->sin_addr) != 1) {
            fprintf(stderr, "[NETWORK-INTERPOSE] Failed to parse device IP: %s\n", device_ip);
        } else {
            fprintf(stderr, "[NETWORK-INTERPOSE] Rewrote %s -> %s\n", original_ip, device_ip);
        }
    }
    // IPv6 rewriting could be added here if needed
}

// Helper function to rewrite port using shared memory mapping
static void rewrite_port(struct sockaddr *addr, socklen_t addrlen) {
    if (addr->sa_family == AF_INET) {
        struct sockaddr_in *addr_in = (struct sockaddr_in *)addr;
        uint16_t original_port = ntohs(addr_in->sin_port);

        if (original_port < PORT_MAP_SIZE && port_map[original_port] != 0) {
            uint16_t new_port = port_map[original_port];
            addr_in->sin_port = htons(new_port);
            fprintf(stderr, "[NETWORK-INTERPOSE] Rewrote port %d -> %d\n", original_port, new_port);
        } else {
            fprintf(stderr, "[NETWORK-INTERPOSE] No mapping found for port %d\n", original_port);
        }
    }
}

// Interposed bind function
int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
    // Lazy initialization
    if (!original_bind) {
        original_bind = dlsym(RTLD_NEXT, "bind");
        if (!original_bind) {
            fprintf(stderr, "[NETWORK-INTERPOSE] Failed to find original bind: %s\n", dlerror());
            errno = EACCES;
            return -1;
        }
    }

    // Check if this is a localhost binding attempt
    if (is_localhost(addr, addrlen)) {
        fprintf(stderr, "[NETWORK-INTERPOSE] Intercepted bind to localhost\n");

        if (strcmp(strategy, STRATEGY_FAIL) == 0) {
            // Strategy A: Fail with error
            if (!is_port_allowed(ntohs(((struct sockaddr_in *)addr)->sin_port))) {
                fprintf(stderr, "[NETWORK-INTERPOSE] Blocking bind to disallowed port\n");
                errno = EACCES;
                return -1;
            }
        } else if (strcmp(strategy, STRATEGY_REWRITE_DEVICE) == 0) {
            // Strategy B: Rewrite to alternative device
            if (listening_loopback_device) {
                struct sockaddr_storage new_addr;
                memcpy(&new_addr, addr, addrlen);
                rewrite_to_device((struct sockaddr *)&new_addr, addrlen, listening_loopback_device);
                fprintf(stderr, "[NETWORK-INTERPOSE] Rewriting bind to device %s\n", listening_loopback_device);
                return original_bind(sockfd, (struct sockaddr *)&new_addr, addrlen);
            }
        } else if (strcmp(strategy, STRATEGY_REWRITE_PORT) == 0) {
            // Strategy C: Rewrite port
            struct sockaddr_storage new_addr;
            memcpy(&new_addr, addr, addrlen);
            rewrite_port((struct sockaddr *)&new_addr, addrlen);
            return original_bind(sockfd, (struct sockaddr *)&new_addr, addrlen);
        }
    }

    return original_bind(sockfd, addr, addrlen);
}

// Interposed connect function
int connect(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
    // Lazy initialization
    if (!original_connect) {
        original_connect = dlsym(RTLD_NEXT, "connect");
        if (!original_connect) {
            fprintf(stderr, "[NETWORK-INTERPOSE] Failed to find original connect: %s\n", dlerror());
            errno = EACCES;
            return -1;
        }
    }

    // Check if this is a localhost connection attempt
    if (is_localhost(addr, addrlen)) {
        fprintf(stderr, "[NETWORK-INTERPOSE] Intercepted connect to localhost\n");

        if (strcmp(strategy, STRATEGY_REWRITE_DEVICE) == 0) {
            // Strategy B: Rewrite to alternative device
            if (connect_loopback_device) {
                struct sockaddr_storage new_addr;
                memcpy(&new_addr, addr, addrlen);
                rewrite_to_device((struct sockaddr *)&new_addr, addrlen, connect_loopback_device);
                fprintf(stderr, "[NETWORK-INTERPOSE] Rewriting connect to device %s\n", connect_loopback_device);
                return original_connect(sockfd, (struct sockaddr *)&new_addr, addrlen);
            }
        } else if (strcmp(strategy, STRATEGY_REWRITE_PORT) == 0) {
            // Strategy C: Rewrite port
            struct sockaddr_storage new_addr;
            memcpy(&new_addr, addr, addrlen);
            rewrite_port((struct sockaddr *)&new_addr, addrlen);
            return original_connect(sockfd, (struct sockaddr *)&new_addr, addrlen);
        }
    }

    return original_connect(sockfd, addr, addrlen);
}

// Initialize port mapping (for Strategy C)
static void initialize_port_map() {
    // Initialize with identity mapping by default
    for (int i = 0; i < PORT_MAP_SIZE; i++) {
        port_map[i] = i;
    }

    // Example mappings - in real implementation this would be loaded from shared memory
    port_map[8080] = 18080;  // Map 8080 -> 18080
    port_map[3000] = 13000;  // Map 3000 -> 13000
}

// Constructor - initialize configuration
__attribute__((constructor))
static void network_interpose_init() {
    strategy = getenv(ENV_STRATEGY);
    if (!strategy) {
        strategy = STRATEGY_FAIL;  // Default to fail strategy
    }

    listening_loopback_device = getenv(ENV_LISTENING_LOOPBACK_DEVICE);
    connect_loopback_device = getenv(ENV_CONNECT_LOOPBACK_DEVICE);

    const char *base_port_str = getenv(ENV_LISTENING_BASE_PORT);
    if (base_port_str) {
        listening_base_port = atoi(base_port_str);
    }

    const char *port_count_str = getenv(ENV_LISTENING_PORT_COUNT);
    if (port_count_str) {
        listening_port_count = atoi(port_count_str);
    }

    initialize_port_map();

    fprintf(stderr, "[NETWORK-INTERPOSE] Initialized with strategy: %s\n", strategy);
    if (listening_loopback_device) {
        fprintf(stderr, "[NETWORK-INTERPOSE] Listening device: %s\n", listening_loopback_device);
    }
    if (connect_loopback_device) {
        fprintf(stderr, "[NETWORK-INTERPOSE] Connect device: %s\n", connect_loopback_device);
    }
    if (listening_base_port >= 0) {
        fprintf(stderr, "[NETWORK-INTERPOSE] Port range: %d-%d\n",
                listening_base_port, listening_base_port + listening_port_count - 1);
    }
}

// Destructor
__attribute__((destructor))
static void network_interpose_cleanup() {
    fprintf(stderr, "[NETWORK-INTERPOSE] Unloaded\n");
}
