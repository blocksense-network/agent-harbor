#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>
#include <dlfcn.h>
#include <string.h>

// Export a test function that can be called to verify loading
__attribute__((visibility("default")))
int dyld_test_verify_loaded() {
    return 42; // Magic number to verify the function is callable
}

// Constructor function - called when library is loaded
__attribute__((constructor))
static void test_library_loaded() {
    // Basic verification that we're in the right process
    pid_t pid = getpid();
    fprintf(stderr, "[DYLD-TEST] Library loaded in process %d\n", pid);

    // Additional verification: check that we can access our own symbols
    void *self = dlopen(NULL, RTLD_NOW);
    if (self) {
        void *sym = dlsym(self, "dyld_test_verify_loaded");
        if (sym) {
            int result = ((int (*)(void))sym)();
            if (result == 42) {
                fprintf(stderr, "[DYLD-TEST] Symbol verification passed in process %d\n", pid);
            } else {
                fprintf(stderr, "[DYLD-TEST] Symbol verification failed (wrong result) in process %d\n", pid);
            }
        } else {
            fprintf(stderr, "[DYLD-TEST] Symbol lookup failed in process %d: %s\n", pid, dlerror());
        }
        dlclose(self);
    } else {
        fprintf(stderr, "[DYLD-TEST] dlopen(NULL) failed in process %d: %s\n", pid, dlerror());
    }

    fflush(stderr);
}

// Optional destructor - called when library is unloaded
__attribute__((destructor))
static void test_library_unloaded() {
    fprintf(stderr, "[DYLD-TEST] Library unloaded from process %d\n", getpid());
    fflush(stderr);
}
