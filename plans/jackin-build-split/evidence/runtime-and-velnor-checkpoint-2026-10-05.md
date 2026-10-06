# Runtime and Velnor checkpoint — 2026-10-05

This checkpoint records two bounded evidence units. It does not report a Jackin build, test, generator invocation, or release qualification.

## Private rootfs Python runtime

The synthetic UID/GID 65534 fixture passed at 2026-10-05 16:47:07 UTC after the fixture corrected its synthetic chroot root from mode 0700 to 0755. The host-side task ancestor remained private at mode 0700. The fixture exercised Python 3.13.5, SHA fallback, and helper preparation only; its synthetic Rustup/Cargo/rustc records were not executed.

The reviewed Python runtime delta was staged at 2026-10-05 16:48:59–16:49:00.877 UTC with this command:

```text
/usr/bin/env -i PATH=/usr/bin:/bin LC_ALL=C /usr/bin/python3 -I -S -B /root/.velnor-generator-preview-20261005/rootfs-python-runtime-delta-v2.py --stage
```

It added 287 regular files, 3 symlinks, and 18 directories (12,032,961 bytes) to the sealed private rootfs. Parent tree SHA-256: `fa4194fa10eecd66c72580c4c6acde1d02a1a9a800275b15178ae5b4bf6acf06`. Result tree SHA-256: `6f7a1176dcd33d0d639c8001787b9b4b78c75df767bd4735f0f033db73a2cd76`.

| Evidence | SHA-256 |
|---|---|
| Synthetic fixture source | `5298066a3a172511de7933055079920d754b41694badbfada0efa286dee7fbed` |
| Fixture result | `b2685574bebc85226568d5d6951ea35a3f5b65648e56579174a4991923877412` |
| Fixture log | `6dcfc123bc64dc75e02b1620e34b8f4af5d65b3d4ed2a846c732879a2b2da04d` |
| Runtime delta source | `d9036d81bb8b178eec7326cebc92dba01b3825c61bafb7b6890396a3342c0850` |
| Delta manifest | `0f28b0d1234c4dee9dc2514483b7162cfe1fb2e0e79dffe7c4786507cf3ea2ad` |
| Stage log | `bf8f53f5961abd1dbec6da0d1a7b664c9d2b3983160fb83bda68dc25e76798c4` |

The manifest labels this state `PYTHON_3_13_MINIMAL_RUNTIME_STAGED; NOT_EXECUTED`. No Rustup, Mise, Cargo, compiler, network, container, account, helper, or generator phase ran after staging. No reversal was run; the sealed rootfs remains preserved for its authorized preview.

## Exact historical Velnor CI run

Velnor commit `86f864aabc2192a9f8ccd3f01f2e25b7858c3f20` (tree `1e16a9270f3ea99b04e68521cfc876fa19db67ac`, parent `aea2cf5bb281ee307b0c00a7faee415486da2bcd`) completed CI run [37340444994](https://github.com/tailrocks/velnor-new/actions/runs/37340444994) successfully. All 21 jobs passed, including Plan, Required, Rust jobs, and Publish baseline. The Plan used Mise 2026.10.1, Rust 1.98.1, and MBX 1.21.1 with:

```text
mise --no-config --no-env --no-hooks exec rust@1.98.1 -- mbx build --release --locked --package velnor-actions-cli --bin velnor-actions
```

Its helper artifact was `velnor-preseed-helper`, ID `11357988774`, API digest `sha256:3504e42f8340adfd597d84ec6afc27260edff0e722da457ef418fe830eec3440`; the plan artifact was `velnor-plan-r37340444994-a1`, ID `11357794273`, digest `sha256:135e6192291ced7cdd327661307b16d9291e841b5d67da787ae09a61363bb21d`. The retrieved helper binary hash was `ef1b12631d057e1607a09ad57ad8db76dbba5f10c09d49015aff4b10cfa5a92e` (13,885,232 bytes). The log records 10 MBX hits, 0 misses, 0 bytes transferred or stored, a 371.7 ms session, and an estimated 2m10s of compiler time avoided. This is same-run CI build/cache evidence, not local helper execution or release qualification.

This CI run is historical: Velnor main later advanced to `540e12a4a225683e78378e63bfdbba2ddba29d86` (tree `3d65a375e6e3431772caff98c7265f7fbc4c6844`). It does not establish checks for that later head.

PR [#65](https://github.com/tailrocks/velnor-new/pull/65) was closed unmerged (`merged_at` and `merge_commit_sha` are null). No `v0.1.1` release exists; the latest semver release is `v0.1.0` (other `generator-*` releases exist). The `generator-release` environment requires reviewer `donbeave` and prevents self-review; the authenticated account is also `donbeave`, so this identity cannot supply the required approval. No bypass or publication occurred. Consequently the v0.1.1 publication gate remains blocked by repository protection, and consumers pinned to that release cannot claim generated-workflow acceptance from this checkpoint.

The Linux ARM image capability also remains absent from the published Velnor schema. Native ARM image build and probe are NOT RUN; the reviewed design packet does not replace a maintained task implementation or hosted CI result.
