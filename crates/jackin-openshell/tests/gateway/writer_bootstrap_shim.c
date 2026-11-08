/* SPDX-License-Identifier: Apache-2.0 */
#define _GNU_SOURCE
#include <dirent.h>
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <sys/types.h>
#if !defined(__APPLE__)
#include <linux/fs.h>
#include <sys/syscall.h>
#endif
#include <unistd.h>

static ssize_t (*next_write)(int, const void *, size_t);
static int (*next_flock)(int, int);
static struct dirent *(*next_readdir)(DIR *);
static int (*next_fstatat)(int, const char *, struct stat *, int);
#if !defined(__APPLE__)
static long (*next_syscall)(long, ...);
#endif
#if !defined(__APPLE__)
static int first_rename_barrier;
#endif
static int first_namespace_lock;
static int selected_temporary;
static int first_stat_barrier;

static void resolve_symbols(void) {
    if (!next_write) next_write = dlsym(RTLD_NEXT, "write");
    if (!next_flock) next_flock = dlsym(RTLD_NEXT, "flock");
    if (!next_readdir) next_readdir = dlsym(RTLD_NEXT, "readdir");
    if (!next_fstatat) next_fstatat = dlsym(RTLD_NEXT, "fstatat");
#if !defined(__APPLE__)
    if (!next_syscall) next_syscall = dlsym(RTLD_NEXT, "syscall");
#endif
}

static ssize_t raw_write(int fd, const void *buffer, size_t length) {
    resolve_symbols();
    if (next_write) return next_write(fd, buffer, length);
    errno = ENOSYS;
    return -1;
}

static int matches_namespace(int fd) {
    const char *namespace = getenv("JACKIN_TEST_NAMESPACE");
    if (!namespace) return 0;
    struct stat opened;
    struct stat named;
    return fstat(fd, &opened) == 0 && stat(namespace, &named) == 0 &&
        S_ISDIR(opened.st_mode) && S_ISDIR(named.st_mode) &&
        opened.st_dev == named.st_dev && opened.st_ino == named.st_ino;
}

static void event(char value) {
    const char *path = getenv("JACKIN_TEST_EVENTS");
    if (!path) return;
    int fd = open(path, O_WRONLY | O_NONBLOCK | O_CLOEXEC);
    if (fd < 0) return;
    raw_write(fd, &value, 1);
    close(fd);
}

static void wait_release(void) {
    const char *path = getenv("JACKIN_TEST_RELEASE");
    if (!path) return;
    int fd = open(path, O_RDONLY | O_CLOEXEC);
    if (fd < 0) return;
    char value;
    while (read(fd, &value, 1) < 0 && errno == EINTR) {}
    close(fd);
}

struct dirent *readdir(DIR *directory) {
    resolve_symbols();
    if (!next_readdir) {
        errno = ENOSYS;
        return NULL;
    }
    struct dirent *entry = next_readdir(directory);
    const char *role = getenv("JACKIN_TEST_ROLE");
    const char *temporary = getenv("JACKIN_TEST_TEMP");
    if (entry && role && strcmp(role, "B") == 0 && temporary &&
        strcmp(entry->d_name, temporary) == 0 && matches_namespace(dirfd(directory))) {
        selected_temporary = 1;
    }
    return entry;
}

int fstatat(int fd, const char *path, struct stat *status, int flags) {
    resolve_symbols();
    if (!next_fstatat) {
        errno = ENOSYS;
        return -1;
    }
    const char *role = getenv("JACKIN_TEST_ROLE");
    const char *temporary = getenv("JACKIN_TEST_TEMP");
    if (role && strcmp(role, "B") == 0 && selected_temporary && !first_stat_barrier &&
        temporary && strcmp(path, temporary) == 0 && (flags & AT_SYMLINK_NOFOLLOW) != 0 &&
        matches_namespace(fd)) {
        first_stat_barrier = 1;
        event('S');
        wait_release();
    }
    return next_fstatat(fd, path, status, flags);
}

int flock(int fd, int operation) {
    resolve_symbols();
    if (!next_flock) {
        errno = ENOSYS;
        return -1;
    }
    const char *role = getenv("JACKIN_TEST_ROLE");
    if (role && strcmp(role, "B") == 0 && !first_namespace_lock &&
        (operation & LOCK_EX) != 0 && matches_namespace(fd)) {
        first_namespace_lock = 1;
        event('D');
    }
    return next_flock(fd, operation);
}

#if !defined(__APPLE__)
long syscall(long number, ...) {
    resolve_symbols();
    if (!next_syscall) {
        errno = ENOSYS;
        return -1;
    }
    if (number != SYS_renameat2) {
        errno = ENOSYS;
        return -1;
    }
    va_list arguments;
    va_start(arguments, number);
    int from_fd = va_arg(arguments, int);
    const char *from = va_arg(arguments, const char *);
    int to_fd = va_arg(arguments, int);
    const char *to = va_arg(arguments, const char *);
    int flags = va_arg(arguments, int);
    va_end(arguments);
    const char *role = getenv("JACKIN_TEST_ROLE");
    if (role && strcmp(role, "A") == 0 && !first_rename_barrier &&
        from && strncmp(from, "identity.", 9) == 0 && to && strcmp(to, "identity") == 0 &&
        flags == RENAME_NOREPLACE && matches_namespace(from_fd) && matches_namespace(to_fd)) {
        first_rename_barrier = 1;
        event('R');
        wait_release();
    }
    return next_syscall(number, from_fd, from, to_fd, to, flags);
}
#endif
