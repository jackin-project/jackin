#define _GNU_SOURCE
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/file.h>
#include <sys/stat.h>
#include <sys/types.h>
#if defined(__APPLE__)
#include <stdio.h>
#else
#include <sys/syscall.h>
#include <linux/fs.h>
#endif
#include <unistd.h>

#ifndef JACKIN_SANDBOX_ROOT
#define JACKIN_SANDBOX_ROOT "/sandbox"
#endif

static int fail(void) {
    return 1;
}

static int unsafe_path(void) {
    return 73;
}

static int write_all(int fd, const unsigned char *buffer, size_t length) {
    while (length > 0) {
        ssize_t written = write(fd, buffer, length);
        if (written < 0 && errno == EINTR) continue;
        if (written <= 0) return -1;
        buffer += (size_t)written;
        length -= (size_t)written;
    }
    return 0;
}

static int random_bytes(unsigned char *buffer, size_t length) {
#if defined(__APPLE__)
    arc4random_buf(buffer, length);
    return 0;
#else
    int fd = open("/dev/urandom", O_RDONLY | O_CLOEXEC);
    if (fd < 0) return -1;
    size_t offset = 0;
    while (offset < length) {
        ssize_t count = read(fd, buffer + offset, length - offset);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0) {
            close(fd);
            return -1;
        }
        offset += (size_t)count;
    }
    return close(fd);
#endif
}

static uint64_t hash_path(const char *path) {
    uint64_t hash = UINT64_C(14695981039346656037);
    for (const unsigned char *p = (const unsigned char *)path; *p; p++) {
        hash ^= *p;
        hash *= UINT64_C(1099511628211);
    }
    return hash;
}

static int open_directory(int parent, const char *name, int create) {
    int fd = openat(parent, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (fd < 0 && (errno == ELOOP || errno == ENOTDIR)) {
        struct stat status;
        if (fstatat(parent, name, &status, AT_SYMLINK_NOFOLLOW) == 0 && S_ISLNK(status.st_mode)) errno = ELOOP;
    }
    if (fd >= 0 || !create || errno != ENOENT) return fd;
    if (mkdirat(parent, name, 0700) < 0 && errno != EEXIST) return -1;
    return openat(parent, name, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
}

static int check_owned_file(int fd, mode_t mode) {
    struct stat status;
    if (fstat(fd, &status) < 0) return -1;
    if (!S_ISREG(status.st_mode) || status.st_uid != getuid() ||
        (status.st_mode & 0777) != mode || status.st_nlink != 1) return -1;
    return 0;
}

static int identity(int dirfd, const char *path) {
    int fd = openat(dirfd, "identity", O_RDONLY | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK);
    if (fd < 0 && errno == ENOENT) return 1;
    if (fd < 0 || check_owned_file(fd, 0600) < 0) {
        if (fd >= 0) close(fd);
        return -1;
    }
    size_t expected = strlen(path);
    size_t offset = 0;
    unsigned char buffer[4096];
    while (offset < expected) {
        size_t amount = expected - offset;
        if (amount > sizeof(buffer)) amount = sizeof(buffer);
        ssize_t count = read(fd, buffer, amount);
        if (count < 0 && errno == EINTR) continue;
        if (count <= 0 || memcmp(buffer, path + offset, (size_t)count) != 0) {
            close(fd);
            return -1;
        }
        offset += (size_t)count;
    }
    unsigned char extra;
    ssize_t count;
    do count = read(fd, &extra, 1); while (count < 0 && errno == EINTR);
    if (close(fd) < 0) return -1;
    return count == 0 ? 0 : -1;
}

static int hex_name(const char *name, size_t prefix_length, size_t hex_length) {
    if (strlen(name) != prefix_length + hex_length) return 0;
    for (size_t i = prefix_length; i < prefix_length + hex_length; i++) {
        if (!((name[i] >= '0' && name[i] <= '9') ||
              (name[i] >= 'a' && name[i] <= 'f'))) return 0;
    }
    return 1;
}

static int inspect_identity_temps(int dirfd, int identity_present, int remove) {
    int copy = openat(dirfd, ".", O_RDONLY | O_DIRECTORY | O_CLOEXEC);
    if (copy < 0) return -1;
    DIR *directory = fdopendir(copy);
    if (!directory) {
        close(copy);
        return -1;
    }
    struct dirent *entry;
    int result = 0;
    int removed = 0;
    while ((entry = readdir(directory)) != NULL) {
        if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0 ||
            strcmp(entry->d_name, "lock") == 0 || strcmp(entry->d_name, "identity") == 0) continue;
        if (strncmp(entry->d_name, "identity.", 9) == 0) {
            if (!hex_name(entry->d_name, 9, 16)) {
                result = -1;
                break;
            }
            struct stat status;
            if (fstatat(dirfd, entry->d_name, &status, AT_SYMLINK_NOFOLLOW) < 0 ||
                !S_ISREG(status.st_mode) || status.st_uid != getuid() ||
                (status.st_mode & 0777) != 0600 || status.st_nlink != 1 ||
                (remove && unlinkat(dirfd, entry->d_name, 0) < 0)) {
                result = -1;
                break;
            }
            removed = remove;
        } else if (!identity_present) {
            result = -1;
            break;
        }
    }
    if (closedir(directory) < 0) result = -1;
    if (result == 0 && removed && fsync(dirfd) < 0) result = -1;
    return result;
}

static int rename_exclusive(int dirfd, const char *from, const char *to) {
#if defined(__APPLE__)
    return renameatx_np(dirfd, from, dirfd, to, RENAME_EXCL);
#else
    return (int)syscall(SYS_renameat2, dirfd, from, dirfd, to, RENAME_NOREPLACE);
#endif
}

static int ensure_identity(int dirfd, const char *path) {
    int present = identity(dirfd, path);
    if (present < 0 || inspect_identity_temps(dirfd, present == 0, 1) < 0) return -1;
    if (present == 0) return 0;
    unsigned char random[8];
    if (random_bytes(random, sizeof(random)) < 0) return -1;
    char temporary[32];
    snprintf(temporary, sizeof(temporary), "identity.%02x%02x%02x%02x%02x%02x%02x%02x",
             random[0], random[1], random[2], random[3], random[4], random[5], random[6], random[7]);
    int fd = openat(dirfd, temporary, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK, 0600);
    if (fd < 0 || check_owned_file(fd, 0600) < 0) {
        if (fd >= 0) close(fd);
        return -1;
    }
    size_t length = strlen(path);
    int result = write_all(fd, (const unsigned char *)path, length);
    if (result == 0 && fsync(fd) < 0) result = -1;
    if (close(fd) < 0) result = -1;
    if (result < 0) return -1;
    if (rename_exclusive(dirfd, temporary, "identity") < 0) return -1;
    if (fsync(dirfd) < 0) return -1;
    return identity(dirfd, path) == 0 ? 0 : -1;
}

static int inspect_namespace(int dirfd, const char *path) {
    int present = identity(dirfd, path);
    if (present < 0) return -1;
    return inspect_identity_temps(dirfd, present == 0, 0);
}

static int cleanup_stages(int dirfd) {
    int copy = dup(dirfd);
    if (copy < 0) return -1;
    DIR *directory = fdopendir(copy);
    if (!directory) {
        close(copy);
        return -1;
    }
    struct dirent *entry;
    int result = 0;
    while ((entry = readdir(directory)) != NULL) {
        if (strlen(entry->d_name) != 21 || strncmp(entry->d_name, "data.", 5) != 0) continue;
        int valid_name = 1;
        for (size_t i = 5; i < 21; i++) {
            if (!((entry->d_name[i] >= '0' && entry->d_name[i] <= '9') ||
                  (entry->d_name[i] >= 'a' && entry->d_name[i] <= 'f'))) valid_name = 0;
        }
        if (!valid_name) continue;
        struct stat status;
        if (fstatat(dirfd, entry->d_name, &status, AT_SYMLINK_NOFOLLOW) < 0 ||
            !S_ISREG(status.st_mode) || status.st_uid != getuid() ||
            (status.st_mode & 0777) != 0600 || status.st_nlink != 1 ||
            unlinkat(dirfd, entry->d_name, 0) < 0) {
            result = -1;
            break;
        }
    }
    if (closedir(directory) < 0) result = -1;
    return result;
}

static int ensure_parent(int root, const char *relative, char **base, int *parent) {
    char *copy = strdup(relative);
    if (!copy) return -1;
    char *last = strrchr(copy, '/');
    *base = strdup(last ? last + 1 : copy);
    if (!*base || **base == '\0' || strcmp(*base, ".") == 0 || strcmp(*base, "..") == 0) {
        free(copy);
        free(*base);
        *base = NULL;
        errno = EINVAL;
        return -1;
    }
    if (last) *last = '\0';
    else copy[0] = '\0';
    int current = dup(root);
    if (current < 0) {
        free(copy);
        free(*base);
        *base = NULL;
        return -1;
    }
    char *cursor = copy;
    while (*cursor) {
        char *slash = strchr(cursor, '/');
        if (slash) *slash = '\0';
        if (*cursor == '\0' || strcmp(cursor, ".") == 0 || strcmp(cursor, "..") == 0) {
            errno = EINVAL;
            goto bad;
        }
        int next = open_directory(current, cursor, 1);
        if (next < 0) goto bad;
        close(current);
        current = next;
        if (!slash) break;
        cursor = slash + 1;
    }
    *parent = current;
    free(copy);
    return 0;
bad:
    close(current);
    free(copy);
    free(*base);
    *base = NULL;
    return -1;
}

int main(int argc, char **argv) {
    if (argc != 4) return fail();
    umask(077);
    char *end = NULL;
    unsigned long mode = strtoul(argv[2], &end, 8);
    if (!end || *end || mode > 0777) return fail();
    errno = 0;
    unsigned long long expected = strtoull(argv[3], &end, 10);
    if (!end || *end || errno || expected > (unsigned long long)INT64_MAX) return fail();
    int root = open(JACKIN_SANDBOX_ROOT, O_RDONLY | O_DIRECTORY | O_NOFOLLOW | O_CLOEXEC);
    if (root < 0) return errno == ELOOP ? unsafe_path() : fail();
    char *base = NULL;
    int parent = -1;
    if (ensure_parent(root, argv[1], &base, &parent) < 0) {
        int error = errno;
        close(root);
        return error == ELOOP || error == EINVAL ? unsafe_path() : fail();
    }
    close(root);
    struct stat target_status;
    int target_result = fstatat(parent, base, &target_status, AT_SYMLINK_NOFOLLOW);
    if (target_result == 0 && S_ISDIR(target_status.st_mode)) {
        close(parent);
        free(base);
        return unsafe_path();
    }
    if (target_result < 0 && errno != ENOENT) {
        close(parent);
        free(base);
        return fail();
    }
    uint64_t hash = hash_path(argv[1]);
    char namespace[48];
    snprintf(namespace, sizeof(namespace), ".jackin-write-%016llx", (unsigned long long)hash);
    if (mkdirat(parent, namespace, 0700) < 0 && errno != EEXIST) goto error;
    int work = open_directory(parent, namespace, 0);
    if (work < 0) goto error;
    struct stat namespace_status;
    if (fstat(work, &namespace_status) < 0 || !S_ISDIR(namespace_status.st_mode) ||
        namespace_status.st_uid != getuid() || (namespace_status.st_mode & 0777) != 0700) {
        close(work);
        goto error;
    }
    if (flock(work, LOCK_EX) < 0) {
        close(work);
        goto error;
    }
    size_t identity_length = strlen(argv[1]);
    size_t root_length = strlen(JACKIN_SANDBOX_ROOT);
    char *identity_path = malloc(root_length + identity_length + 2);
    if (!identity_path) {
        close(work);
        goto error;
    }
    snprintf(identity_path, root_length + identity_length + 2, "%s/%s", JACKIN_SANDBOX_ROOT, argv[1]);
    if (inspect_namespace(work, identity_path) < 0) {
        free(identity_path);
        close(work);
        goto error;
    }
    int lock = openat(work, "lock", O_CREAT | O_RDWR | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK, 0600);
    if (lock < 0 || check_owned_file(lock, 0600) < 0 || flock(lock, LOCK_EX) < 0) {
        if (lock >= 0) close(lock);
        free(identity_path);
        close(work);
        goto error;
    }
    if (ensure_identity(work, identity_path) < 0 || cleanup_stages(work) < 0) {
        free(identity_path);
        goto locked_error;
    }
    free(identity_path);
    unsigned char random[16];
    if (random_bytes(random, sizeof(random)) < 0) goto locked_error;
    char stage[48];
    snprintf(stage, sizeof(stage), "data.%02x%02x%02x%02x%02x%02x%02x%02x", random[0], random[1], random[2], random[3], random[4], random[5], random[6], random[7]);
    int data = openat(work, stage, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC | O_NONBLOCK, 0600);
    if (data < 0 || check_owned_file(data, 0600) < 0) {
        if (data >= 0) close(data);
        goto locked_error;
    }
    unsigned long long total = 0;
    unsigned char buffer[65536];
    int result = 0;
    for (;;) {
        ssize_t count = read(STDIN_FILENO, buffer, sizeof(buffer));
        if (count < 0 && errno == EINTR) continue;
        if (count < 0) {
            result = -1;
            break;
        }
        if (count == 0) break;
        if ((unsigned long long)count > expected - total || write_all(data, buffer, (size_t)count) < 0) {
            result = -1;
            break;
        }
        total += (unsigned long long)count;
    }
    struct stat status;
    if (result == 0 && (total != expected || fstat(data, &status) < 0 ||
                        (unsigned long long)status.st_size != expected || fchmod(data, (mode_t)mode) < 0 || fsync(data) < 0)) result = -1;
    if (close(data) < 0) result = -1;
    if (result == 0 && renameat(work, stage, parent, base) < 0) result = -1;
    if (result != 0) unlinkat(work, stage, 0);
    flock(lock, LOCK_UN);
    close(lock);
    close(work);
    close(parent);
    free(base);
    return result == 0 ? 0 : 1;
locked_error:
    flock(lock, LOCK_UN);
    close(lock);
    close(work);
error:
    close(parent);
    free(base);
    return fail();
}
