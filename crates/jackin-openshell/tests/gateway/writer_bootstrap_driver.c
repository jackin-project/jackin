/* SPDX-License-Identifier: Apache-2.0 */
#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <poll.h>
#include <signal.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/resource.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>

struct Child {
    pid_t pid;
    char role;
    int status;
};

static int events_fd = -1;
static int release_fd = -1;
static atomic_int event_error;

static void event(char value) {
    if (write(events_fd, &value, 1) != 1) atomic_store(&event_error, 1);
}

static void *watch_child(void *argument) {
    struct Child *child = argument;
    int status = 0;
    pid_t result;
    do result = waitpid(child->pid, &status, 0); while (result < 0 && errno == EINTR);
    if (result != child->pid) atomic_store(&event_error, 1);
    child->status = status;
    event(child->role == 'A' ? 'a' : 'b');
    return NULL;
}

static int write_all(int fd, const char *text) {
    size_t length = strlen(text);
    size_t offset = 0;
    while (offset < length) {
        ssize_t count = write(fd, text + offset, length - offset);
        if (count < 0 && errno == EINTR) continue;
        if (count < 0) return -1;
        if (count == 0) {
            errno = EIO;
            return -1;
        }
        offset += (size_t)count;
    }
    return 0;
}

static int spawn_writer(struct Child *child, const char *helper, const char *shim,
                        const char *namespace, const char *events, const char *release,
                        const char *temporary, const char *relative, const char *payload,
                        int limited) {
    int input[2];
    if (pipe(input) < 0) return -1;
    child->pid = fork();
    if (child->pid < 0) {
        close(input[0]);
        close(input[1]);
        return -1;
    }
    if (child->pid == 0) {
        close(input[1]);
        if (dup2(input[0], STDIN_FILENO) < 0) _exit(120);
        close(input[0]);
        if (limited) {
            struct rlimit limit = {.rlim_cur = 1, .rlim_max = 1};
            if (setrlimit(RLIMIT_FSIZE, &limit) < 0) _exit(121);
            int null = open("/dev/null", O_WRONLY | O_CLOEXEC);
            if (null < 0 || dup2(null, STDOUT_FILENO) < 0 || dup2(null, STDERR_FILENO) < 0) _exit(122);
            close(null);
        } else {
#if defined(__APPLE__)
            setenv("DYLD_INSERT_LIBRARIES", shim, 1);
#else
            setenv("LD_PRELOAD", shim, 1);
#endif
            setenv("JACKIN_TEST_ROLE", child->role == 'A' ? "A" : "B", 1);
            setenv("JACKIN_TEST_NAMESPACE", namespace, 1);
            setenv("JACKIN_TEST_EVENTS", events, 1);
            setenv("JACKIN_TEST_RELEASE", release, 1);
            if (temporary) setenv("JACKIN_TEST_TEMP", temporary, 1);
        }
        char mode[] = "0600";
        char length[32];
        snprintf(length, sizeof(length), "%zu", strlen(payload));
        execl(helper, helper, relative, mode, length, (char *)NULL);
        _exit(123);
    }
    close(input[0]);
    int result = write_all(input[1], payload);
    int error = errno;
    close(input[1]);
    return result == 0 || error == EPIPE ? 0 : -1;
}

static int next_event(char *value) {
    struct pollfd descriptor = {.fd = events_fd, .events = POLLIN};
    for (;;) {
        int result = poll(&descriptor, 1, 15000);
        if (result < 0 && errno == EINTR) continue;
        if (result <= 0) return -1;
        ssize_t count = read(events_fd, value, 1);
        if (count == 1) return 0;
        if (count < 0 && (errno == EAGAIN || errno == EINTR)) continue;
        return -1;
    }
}

static int wait_for_event(char expected) {
    char value = 0;
    if (next_event(&value) < 0) return -1;
    return value == expected ? 0 : -1;
}

static int release_tokens(size_t count) {
    char tokens[32];
    if (count > sizeof(tokens)) return -1;
    memset(tokens, 'R', count);
    size_t offset = 0;
    while (offset < count) {
        ssize_t written = write(release_fd, tokens + offset, count - offset);
        if (written < 0 && errno == EINTR) continue;
        if (written < 0) return -1;
        if (written == 0) return -1;
        offset += (size_t)written;
    }
    return 0;
}

static int valid_temporary_name(const char *name) {
    if (strlen(name) != 25 || strncmp(name, "identity.", 9) != 0) return 0;
    for (size_t index = 9; index < 25; index++) {
        if (!((name[index] >= '0' && name[index] <= '9') ||
              (name[index] >= 'a' && name[index] <= 'f'))) return 0;
    }
    return 1;
}

static int valid_identity_file(const char *namespace, const char *temporary,
                               const char *identity) {
    char path[4096];
    if (snprintf(path, sizeof(path), "%s/%s", namespace, temporary) >= (int)sizeof(path)) return -1;
    struct stat named;
    if (lstat(path, &named) < 0 || !S_ISREG(named.st_mode) || named.st_uid != getuid() ||
        (named.st_mode & 0777) != 0600 || named.st_nlink != 1) return -1;
    int fd = open(path, O_RDONLY | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0) return -1;
    struct stat opened;
    if (fstat(fd, &opened) < 0 || opened.st_dev != named.st_dev || opened.st_ino != named.st_ino ||
        !S_ISREG(opened.st_mode) || opened.st_uid != getuid() ||
        (opened.st_mode & 0777) != 0600 || opened.st_nlink != 1) {
        close(fd);
        return -1;
    }
    size_t expected = strlen(identity);
    size_t offset = 0;
    char buffer[4096];
    while (offset < expected) {
        size_t amount = expected - offset;
        if (amount > sizeof(buffer)) amount = sizeof(buffer);
        ssize_t count = read(fd, buffer, amount);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0 || memcmp(buffer, identity + offset, (size_t)count) != 0) {
            close(fd);
            return -1;
        }
        offset += (size_t)count;
    }
    char extra;
    ssize_t count;
    do count = read(fd, &extra, 1); while (count < 0 && errno == EINTR);
    int close_result = close(fd);
    return count == 0 && close_result == 0 ? 0 : -1;
}

static int find_temporary(const char *namespace, const char *identity,
                          char *temporary, size_t temporary_size) {
    DIR *directory = opendir(namespace);
    if (!directory) return -1;
    struct dirent *entry;
    size_t found = 0;
    int identity_found = 0;
    int result = 0;
    while ((entry = readdir(directory)) != NULL) {
        if (strcmp(entry->d_name, "identity") == 0) identity_found = 1;
        if (!valid_temporary_name(entry->d_name)) continue;
        found++;
        if (strlen(entry->d_name) >= temporary_size) {
            result = -1;
            break;
        }
        strcpy(temporary, entry->d_name);
    }
    if (closedir(directory) < 0) result = -1;
    if (result < 0 || identity_found || found != 1) return -1;
    return valid_identity_file(namespace, temporary, identity);
}

static int wait_for_child(struct Child *child, pthread_t watcher, int watched,
                          int *reaped) {
    if (watched) {
        if (pthread_join(watcher, NULL) != 0) return -1;
        *reaped = 1;
        return 0;
    }
    pid_t result;
    do result = waitpid(child->pid, &child->status, 0); while (result < 0 && errno == EINTR);
    if (result != child->pid) return -1;
    *reaped = 1;
    return 0;
}

static int limit_probe(const char *helper, const char *relative, const char *payload) {
    struct Child child = {.role = 'C', .status = 0};
    if (spawn_writer(&child, helper, NULL, NULL, NULL, NULL, NULL, relative, payload, 1) < 0) return 1;
    int status = 0;
    while (waitpid(child.pid, &status, 0) < 0 && errno == EINTR) {}
    if (WIFSIGNALED(status) && WTERMSIG(status) == SIGXFSZ) {
        puts("termination=SIGXFSZ");
        return 0;
    }
    printf("termination=unexpected status=%d\n", status);
    return 1;
}

static int concurrent_probe(const char *helper, const char *shim, const char *root,
                            const char *namespace, const char *relative,
                            const char *first_payload, const char *second_payload) {
    char events[4096];
    char release[4096];
    char identity[4096];
    char temporary[64] = {0};
    if (snprintf(events, sizeof(events), "%s/events", root) >= (int)sizeof(events) ||
        snprintf(release, sizeof(release), "%s/release", root) >= (int)sizeof(release) ||
        snprintf(identity, sizeof(identity), "%s/sandbox/%s", root, relative) >= (int)sizeof(identity)) return 3;
    unlink(events);
    unlink(release);
    if (mkfifo(events, 0600) < 0 || mkfifo(release, 0600) < 0) return 4;
    events_fd = open(events, O_RDWR | O_NONBLOCK | O_CLOEXEC);
    release_fd = open(release, O_RDWR | O_NONBLOCK | O_CLOEXEC);
    if (events_fd < 0 || release_fd < 0) return 5;
    struct Child first = {.role = 'A', .status = 0};
    struct Child second = {.role = 'B', .status = 0};
    pthread_t first_watcher;
    pthread_t second_watcher;
    int first_started = 0;
    int second_started = 0;
    int first_watched = 0;
    int second_watched = 0;
    int first_reaped = 0;
    int second_reaped = 0;
    int releases_sent = 0;
    int result = 1;
    if (spawn_writer(&first, helper, shim, namespace, events, release, NULL,
                     relative, first_payload, 0) < 0) {
        first_started = first.pid > 0;
        goto done;
    }
    first_started = 1;
    if (pthread_create(&first_watcher, NULL, watch_child, &first) != 0) goto done;
    first_watched = 1;
    if (wait_for_event('R') < 0) goto done;
    if (find_temporary(namespace, identity, temporary, sizeof(temporary)) < 0) {
        puts("identity_temporary=invalid");
        goto done;
    }
    if (spawn_writer(&second, helper, shim, namespace, events, release, temporary,
                     relative, second_payload, 0) < 0) {
        second_started = second.pid > 0;
        goto done;
    }
    second_started = 1;
    if (pthread_create(&second_watcher, NULL, watch_child, &second) != 0) goto done;
    second_watched = 1;
    char boundary = 0;
    if (next_event(&boundary) < 0 || (boundary != 'D' && boundary != 'S')) goto done;
    printf("second_boundary=%c\n", boundary);
    if (release_tokens(1) < 0) goto done;
    releases_sent = 1;
    if (wait_for_event('a') < 0 || wait_for_child(&first, first_watcher, first_watched,
                                                  &first_reaped) < 0) goto done;
    first_watched = 0;
    if (atomic_load(&event_error) || !WIFEXITED(first.status) || WEXITSTATUS(first.status) != 0) {
        printf("status_a=%d\n", first.status);
        goto done;
    }
    if (boundary == 'S' && release_tokens(1) < 0) goto done;
    if (wait_for_event('b') < 0 || wait_for_child(&second, second_watcher, second_watched,
                                                   &second_reaped) < 0) goto done;
    second_watched = 0;
    printf("status_a=%d status_b=%d\n", first.status, second.status);
    if (atomic_load(&event_error) || !WIFEXITED(second.status) || WEXITSTATUS(second.status) != 0) goto done;
    result = 0;
done:
    if (!releases_sent || !first_reaped || !second_reaped) release_tokens(16);
    if (first_started && !first_reaped) {
        if (first_watched) {
            pthread_join(first_watcher, NULL);
            first_watched = 0;
        } else {
            while (waitpid(first.pid, &first.status, 0) < 0 && errno == EINTR) {}
        }
        first_reaped = 1;
    }
    if (second_started && !second_reaped) {
        if (second_watched) {
            pthread_join(second_watcher, NULL);
            second_watched = 0;
        } else {
            while (waitpid(second.pid, &second.status, 0) < 0 && errno == EINTR) {}
        }
        second_reaped = 1;
    }
    if (events_fd >= 0) close(events_fd);
    if (release_fd >= 0) close(release_fd);
    unlink(events);
    unlink(release);
    return result;
}

int main(int argc, char **argv) {
    signal(SIGPIPE, SIG_IGN);
    if (argc == 5 && strcmp(argv[1], "limit") == 0) {
        return limit_probe(argv[2], argv[3], argv[4]);
    }
    if (argc != 9 || strcmp(argv[1], "concurrent") != 0) return 2;
    return concurrent_probe(argv[2], argv[3], argv[4], argv[5], argv[6], argv[7], argv[8]);
}
