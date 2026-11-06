import ctypes, os, socket, errno
libc = ctypes.CDLL(None, use_errno=True)
def r(name, rc):
    e = ctypes.get_errno()
    print(f"{name}: rc={rc} errno={errno.errorcode.get(e, e) if rc != 0 else '-'}")
CLONE_NEWNS, CLONE_NEWUSER, CLONE_NEWNET, CLONE_NEWPID = 0x20000, 0x10000000, 0x40000000, 0x20000000
for n, f in (("unshare(NEWUSER)", CLONE_NEWUSER), ("unshare(NEWNS)", CLONE_NEWNS), ("unshare(NEWNET)", CLONE_NEWNET), ("unshare(NEWPID)", CLONE_NEWPID)):
    pid = os.fork()
    if pid == 0:
        rc = libc.unshare(f); e = ctypes.get_errno()
        print(f"{n}: rc={rc} errno={errno.errorcode.get(e, e) if rc else '-'}", flush=True)
        os._exit(0)
    os.waitpid(pid, 0)
os.makedirs("/tmp/mnt-probe", exist_ok=True)
r("mount(tmpfs)", libc.mount(b"none", b"/tmp/mnt-probe", b"tmpfs", 0, None))
r("setns", libc.setns(-1, 0))
r("pivot_root", libc.syscall(41, b"/tmp", b"/tmp"))
for name, proto in (("NETLINK_ROUTE", 0), ("NETLINK_NETFILTER", 12), ("NETLINK_GENERIC", 16), ("NETLINK_SOCK_DIAG", 4)):
    try:
        s = socket.socket(socket.AF_NETLINK, socket.SOCK_RAW, proto); print(f"socket(AF_NETLINK,{name}): ok"); s.close()
    except OSError as ex:
        print(f"socket(AF_NETLINK,{name}): {errno.errorcode[ex.errno]}")
for name, fam, typ in (("AF_PACKET", socket.AF_PACKET, socket.SOCK_RAW), ("AF_INET", socket.AF_INET, socket.SOCK_STREAM)):
    try:
        s = socket.socket(fam, typ); print(f"socket({name}): ok"); s.close()
    except OSError as ex:
        print(f"socket({name}): {errno.errorcode[ex.errno]}")
print("memfd_create:", libc.syscall(279, b"x", 0), errno.errorcode.get(ctypes.get_errno(), ctypes.get_errno()))
print("bpf:", libc.syscall(280, 0, 0, 0), errno.errorcode.get(ctypes.get_errno(), ctypes.get_errno()))
print("ptrace:", libc.ptrace(0, 0, 0, 0), errno.errorcode.get(ctypes.get_errno(), ctypes.get_errno()))
print("mknod /tmp/n:", end=" ")
try:
    os.mknod("/tmp/n-probe", 0o600 | 0o020000, os.makedev(1,3)); print("ok")
except OSError as ex: print(errno.errorcode[ex.errno])
for l in open("/proc/self/status"):
    if l.split(":")[0] in ("Uid","CapEff","CapBnd","NoNewPrivs","Seccomp","Seccomp_filters"): print(l.strip())
