/*
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <fcntl.h>
#include <sys/stat.h>
#include <dirent.h>
#include <errno.h>
#include <dlfcn.h>
#include <pthread.h>
#include <stdarg.h>
#include <sys/socket.h>
#include <sys/un.h>
#include <arpa/inet.h>
#include <string.h>
#include <errno.h>

// Environment variables
#define ENV_AGENTFS_SERVER "AGENTFS_SERVER"
#define ENV_AGENTFS_ENABLED "AGENTFS_ENABLED"

// Socket path suffix for JSON protocol (C client uses JSON)
#define JSON_SOCKET_SUFFIX ".json"

// Global state
static int agentfs_enabled = 0;
static char *agentfs_server = NULL;
static pthread_key_t client_key;

// Simplified synchronous AgentFS client for PoC
// In a real implementation, this would be a proper async client
typedef struct AgentFsClient {
    int sockfd;
    int next_local_handle;
} AgentFsClient;

static int send_request(int sockfd, const char* json_request) {
    size_t len = strlen(json_request);
    uint32_t net_len = htonl(len);

    if (write(sockfd, &net_len, sizeof(net_len)) != sizeof(net_len)) {
        return -1;
    }

    if (write(sockfd, json_request, len) != len) {
        return -1;
    }

    return 0;
}

static char* receive_response(int sockfd) {
    uint32_t net_len;
    if (read(sockfd, &net_len, sizeof(net_len)) != sizeof(net_len)) {
        return NULL;
    }

    size_t len = ntohl(net_len);
    char* buffer = malloc(len + 1);
    if (!buffer) return NULL;

    if (read(sockfd, buffer, len) != len) {
        free(buffer);
        return NULL;
    }

    buffer[len] = '\0';
    return buffer;
}

AgentFsClient* agentfs_client_connect(const char* socket_path) {
    fprintf(stderr, "[FS-INTERPOSE] Attempting to connect to: %s\n", socket_path);
    int sockfd = socket(AF_UNIX, SOCK_STREAM, 0);
    if (sockfd < 0) {
        fprintf(stderr, "[FS-INTERPOSE] Failed to create socket: %s\n", strerror(errno));
        return NULL;
    }

    struct sockaddr_un addr;
    memset(&addr, 0, sizeof(addr));
    addr.sun_family = AF_UNIX;
    strncpy(addr.sun_path, socket_path, sizeof(addr.sun_path) - 1);

    if (connect(sockfd, (struct sockaddr*)&addr, sizeof(addr)) < 0) {
        fprintf(stderr, "[FS-INTERPOSE] Failed to connect to AgentFS server %s: %s\n", socket_path, strerror(errno));
        close(sockfd);
        return NULL;
    }

    AgentFsClient* client = malloc(sizeof(AgentFsClient));
    if (!client) {
        close(sockfd);
        return NULL;
    }

    client->sockfd = sockfd;
    client->next_local_handle = 1;

    fprintf(stderr, "[FS-INTERPOSE] Successfully connected to AgentFS server: %s\n", socket_path);
    return client;
}

void agentfs_client_disconnect(AgentFsClient* client) {
    if (client) {
        close(client->sockfd);
        free(client);
    }
}

int agentfs_client_open(AgentFsClient* client, const char* path, int flags) {
    if (!client) return -1;

    // Simplified: assume read/write access
    int read = (flags & O_RDONLY) || (flags & O_RDWR);
    int write = (flags & O_WRONLY) || (flags & O_RDWR);
    int create = (flags & O_CREAT);

    char json[1024];
    if (create) {
        snprintf(json, sizeof(json),
            "{\"version\":\"1\",\"op\":\"fs.create\",\"path\":\"%s\",\"read\":%s,\"write\":%s}",
            path, read ? "true" : "false", write ? "true" : "false");
    } else {
        snprintf(json, sizeof(json),
            "{\"version\":\"1\",\"op\":\"fs.open\",\"path\":\"%s\",\"read\":%s,\"write\":%s,\"create\":false}",
            path, read ? "true" : "false", write ? "true" : "false");
    }

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    // Parse response - simplified parsing
    if (strstr(response, "\"handle\":")) {
        int handle = client->next_local_handle++;
        free(response);
        return handle;
    }

    free(response);
    return -1;
}

int agentfs_client_close(AgentFsClient* client, int fd) {
    if (!client) return -1;

    char json[256];
    snprintf(json, sizeof(json), "{\"version\":\"1\",\"op\":\"fs.close\",\"handle\":%d}", fd);

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    int success = strstr(response, "\"Ok\"") != NULL;
    free(response);

    return success ? 0 : -1;
}

ssize_t agentfs_client_read(AgentFsClient* client, int fd, void* buf, size_t count, off_t offset) {
    if (!client || count > 65536) return -1; // Reasonable limit

    char json[256];
    snprintf(json, sizeof(json),
        "{\"version\":\"1\",\"op\":\"fs.read\",\"handle\":%d,\"offset\":%lld,\"len\":%zu}",
        fd, (long long)offset, count);

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    // Parse JSON response - simplified: look for "data":"base64" or similar
    char* data_start = strstr(response, "\"data\":");
    if (data_start) {
        // Very simplified parsing - in real implementation use proper JSON parser
        char* bracket = strchr(data_start, '[');
        if (bracket) {
            // For now, just return some dummy data to show it works
            memset(buf, 'X', count > 10 ? 10 : count);
            free(response);
            return count > 10 ? 10 : count;
        }
    }

    free(response);
    return -1;
}

ssize_t agentfs_client_write(AgentFsClient* client, int fd, const void* buf, size_t count, off_t offset) {
    if (!client) return -1;

    // Simplified: don't send actual data in PoC
    char json[256];
    snprintf(json, sizeof(json),
        "{\"version\":\"1\",\"op\":\"fs.write\",\"handle\":%d,\"offset\":%lld,\"data\":[]}",
        fd, (long long)offset);

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    // Check for success
    int success = strstr(response, "\"len\":") != NULL;
    free(response);

    return success ? count : -1;
}

int agentfs_client_getattr(AgentFsClient* client, const char* path, struct stat* st) {
    if (!client) return -1;

    char json[512];
    snprintf(json, sizeof(json), "{\"version\":\"1\",\"op\":\"fs.getattr\",\"path\":\"%s\"}", path);

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    // Simplified parsing
    if (strstr(response, "\"len\":")) {
        // Fill in some dummy stat info
        memset(st, 0, sizeof(struct stat));
        st->st_mode = S_IFREG | 0644;
        st->st_size = 1024; // Dummy size
        free(response);
        return 0;
    }

    free(response);
    return -1;
}

int agentfs_client_mkdir(AgentFsClient* client, const char* path) {
    if (!client) return -1;

    char json[512];
    snprintf(json, sizeof(json), "{\"version\":\"1\",\"op\":\"fs.mkdir\",\"path\":\"%s\"}", path);

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    int success = strstr(response, "\"Ok\"") != NULL;
    free(response);

    return success ? 0 : -1;
}

int agentfs_client_unlink(AgentFsClient* client, const char* path) {
    if (!client) return -1;

    char json[512];
    snprintf(json, sizeof(json), "{\"version\":\"1\",\"op\":\"fs.unlink\",\"path\":\"%s\"}", path);

    if (send_request(client->sockfd, json) < 0) {
        return -1;
    }

    char* response = receive_response(client->sockfd);
    if (!response) {
        return -1;
    }

    int success = strstr(response, "\"Ok\"") != NULL;
    free(response);

    return success ? 0 : -1;
}

// Original function pointers
static int (*original_open)(const char *, int, ...) = NULL;
static int (*original_close)(int) = NULL;
static ssize_t (*original_read)(int, void *, size_t) = NULL;
static ssize_t (*original_write)(int, const void *, size_t) = NULL;
static int (*original_stat)(const char *, struct stat *) = NULL;
static int (*original_lstat)(const char *, struct stat *) = NULL;
static int (*original_mkdir)(const char *, mode_t) = NULL;
static int (*original_unlink)(const char *) = NULL;
static DIR* (*original_opendir)(const char *) = NULL;
static struct dirent* (*original_readdir)(DIR *) = NULL;
static int (*original_closedir)(DIR *) = NULL;

// Get AgentFS client for current thread
static AgentFsClient* get_client() {
    AgentFsClient* client = pthread_getspecific(client_key);
    if (!client && agentfs_server) {
        // Use JSON socket for C client (C client uses JSON protocol)
        char json_socket_path[256];
        snprintf(json_socket_path, sizeof(json_socket_path), "%s%s", agentfs_server, JSON_SOCKET_SUFFIX);
        client = agentfs_client_connect(json_socket_path);
        if (client) {
            pthread_setspecific(client_key, client);
        }
    }
    return client;
}

// Check if path should be handled by AgentFS
// For PoC, we'll handle paths starting with "/agentfs/"
static int should_handle_path(const char* path) {
    return path && strncmp(path, "/agentfs/", 9) == 0;
}

// Interposed open function
int open(const char *pathname, int flags, ...) {
    mode_t mode = 0;
    if (flags & O_CREAT) {
        va_list args;
        va_start(args, flags);
        mode = va_arg(args, mode_t);
        va_end(args);
    }

    if (!original_open) {
        original_open = dlsym(RTLD_NEXT, "open");
    }

    if (agentfs_enabled && should_handle_path(pathname)) {
        AgentFsClient* client = get_client();
        if (client) {
            int fd = agentfs_client_open(client, pathname, flags);
            if (fd >= 0) {
                return fd;
            }
            // Fall back to original on error
        }
    }

    if (flags & O_CREAT) {
        return original_open(pathname, flags, mode);
    } else {
        return original_open(pathname, flags);
    }
}

// Interposed close function
int close(int fd) {
    if (!original_close) {
        original_close = dlsym(RTLD_NEXT, "close");
    }

    AgentFsClient* client = get_client();
    if (client && agentfs_client_close(client, fd) == 0) {
        return 0;
    }

    return original_close(fd);
}

// Interposed read function (simplified, doesn't handle offset)
ssize_t read(int fd, void *buf, size_t count) {
    if (!original_read) {
        original_read = dlsym(RTLD_NEXT, "read");
    }

    AgentFsClient* client = get_client();
    if (client) {
        ssize_t result = agentfs_client_read(client, fd, buf, count, -1);
        if (result >= 0) {
            return result;
        }
        // Fall back to original on error
    }

    return original_read(fd, buf, count);
}

// Interposed write function (simplified, doesn't handle offset)
ssize_t write(int fd, const void *buf, size_t count) {
    if (!original_write) {
        original_write = dlsym(RTLD_NEXT, "write");
    }

    AgentFsClient* client = get_client();
    if (client) {
        ssize_t result = agentfs_client_write(client, fd, buf, count, -1);
        if (result >= 0) {
            return result;
        }
        // Fall back to original on error
    }

    return original_write(fd, buf, count);
}

// Interposed stat functions
int stat(const char *pathname, struct stat *statbuf) {
    if (!original_stat) {
        original_stat = dlsym(RTLD_NEXT, "stat");
    }

    if (agentfs_enabled && should_handle_path(pathname)) {
        AgentFsClient* client = get_client();
        if (client && agentfs_client_getattr(client, pathname, statbuf) == 0) {
            return 0;
        }
    }

    return original_stat(pathname, statbuf);
}

int lstat(const char *pathname, struct stat *statbuf) {
    if (!original_lstat) {
        original_lstat = dlsym(RTLD_NEXT, "lstat");
    }

    if (agentfs_enabled && should_handle_path(pathname)) {
        AgentFsClient* client = get_client();
        if (client && agentfs_client_getattr(client, pathname, statbuf) == 0) {
            return 0;
        }
    }

    return original_lstat(pathname, statbuf);
}

// Interposed mkdir function
int mkdir(const char *pathname, mode_t mode) {
    if (!original_mkdir) {
        original_mkdir = dlsym(RTLD_NEXT, "mkdir");
    }

    if (agentfs_enabled && should_handle_path(pathname)) {
        AgentFsClient* client = get_client();
        if (client && agentfs_client_mkdir(client, pathname) == 0) {
            return 0;
        }
    }

    return original_mkdir(pathname, mode);
}

// Interposed unlink function
int unlink(const char *pathname) {
    if (!original_unlink) {
        original_unlink = dlsym(RTLD_NEXT, "unlink");
    }

    if (agentfs_enabled && should_handle_path(pathname)) {
        AgentFsClient* client = get_client();
        if (client && agentfs_client_unlink(client, pathname) == 0) {
            return 0;
        }
    }

    return original_unlink(pathname);
}

// Directory operations (simplified)
DIR *opendir(const char *name) {
    if (!original_opendir) {
        original_opendir = dlsym(RTLD_NEXT, "opendir");
    }

    // For PoC, we don't intercept directory operations yet
    // They would require more complex state management
    return original_opendir(name);
}

struct dirent *readdir(DIR *dirp) {
    if (!original_readdir) {
        original_readdir = dlsym(RTLD_NEXT, "readdir");
    }

    return original_readdir(dirp);
}

int closedir(DIR *dirp) {
    if (!original_closedir) {
        original_closedir = dlsym(RTLD_NEXT, "closedir");
    }

    return original_closedir(dirp);
}

// Cleanup function for thread-local storage
static void client_cleanup(void* client) {
    if (client) {
        agentfs_client_disconnect(client);
    }
}

// Initialize interposition
__attribute__((constructor))
static void fs_interpose_init() {
    const char* enabled = getenv(ENV_AGENTFS_ENABLED);
    agentfs_enabled = enabled && strcmp(enabled, "1") == 0;

    agentfs_server = getenv(ENV_AGENTFS_SERVER);

    if (agentfs_enabled) {
        fprintf(stderr, "[FS-INTERPOSE] Enabled, server: %s\n", agentfs_server ?: "none");

        // Initialize thread-local storage for clients
        pthread_key_create(&client_key, client_cleanup);
    } else {
        fprintf(stderr, "[FS-INTERPOSE] Disabled\n");
    }
}

// Cleanup
__attribute__((destructor))
static void fs_interpose_cleanup() {
    if (agentfs_enabled) {
        // Clean up thread-local storage
        pthread_key_delete(client_key);
        fprintf(stderr, "[FS-INTERPOSE] Unloaded\n");
    }
}
