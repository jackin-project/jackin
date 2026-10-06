// SPDX-FileCopyrightText: 2026 Alexey Zhokhov
// SPDX-License-Identifier: Apache-2.0

use super::*;
pub(super) const CAPSULE_VERSION: &str = "0.6.4-preview.1+0123456";

pub(super) fn git(directory: &Path, args: &[&str]) -> String {
    let mut command = crate::cmd::command("git");
    command.arg("-C").arg(directory).args(args);
    String::from_utf8(crate::cmd::output(&mut command).unwrap())
        .unwrap()
        .trim()
        .to_owned()
}

pub(super) fn source_manifest(source_commit: String) -> PackageManifest {
    PackageManifest {
        assets: Vec::new(),
        schema: MANIFEST_SCHEMA.to_owned(),
        source_commit,
        source_ref: SOURCE_REF.to_owned(),
        source_repository: SOURCE_REPOSITORY.to_owned(),
        supporting_assets: Vec::new(),
        version: String::new(),
    }
}

pub(super) fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub(super) fn write_payloads(directory: &Path) -> BTreeMap<String, String> {
    PAYLOADS
        .iter()
        .enumerate()
        .map(|(index, payload)| {
            let bytes = format!("payload-{index}").into_bytes();
            let path = directory.join(payload.name);
            fs::write(&path, &bytes).unwrap();
            (payload.name.to_owned(), digest(&bytes))
        })
        .collect()
}

pub(super) fn release_archive(path: &Path, members: &[(&str, &[u8], u32, EntryType)]) {
    let encoder = GzEncoder::new(fs::File::create(path).unwrap(), Compression::default());
    let mut archive = TarBuilder::new(encoder);
    for (name, bytes, mode, entry_type) in members {
        let mut header = Header::new_gnu();
        let path = name.as_bytes();
        assert!(
            path.len() < 100,
            "test member path must fit the tar name field"
        );
        header.as_mut_bytes()[..path.len()].copy_from_slice(path);
        header.set_size(bytes.len() as u64);
        header.set_mode(*mode);
        header.set_mtime(0);
        header.set_entry_type(*entry_type);
        header.set_cksum();
        archive.append(&header, *bytes).unwrap();
    }
    archive.into_inner().unwrap().finish().unwrap();
}

pub(super) fn elf64_x86_64_version_binary(binary: &str, version: &str) -> Vec<u8> {
    const ELF_HEADER_SIZE: usize = 64;
    const PROGRAM_HEADER_SIZE: usize = 56;
    const CODE_OFFSET: usize = ELF_HEADER_SIZE + PROGRAM_HEADER_SIZE;

    let message = format!("{binary} {version}\n").into_bytes();
    let message_len = u8::try_from(message.len()).expect("ELF fixture message fits in u8");
    let code = [
        0x48,
        0x8d,
        0x35,
        0x1a,
        0x00,
        0x00,
        0x00, // lea message(%rip), %rsi
        0xba,
        message_len,
        0x00,
        0x00,
        0x00, // mov message length, %edx
        0xbf,
        0x01,
        0x00,
        0x00,
        0x00, // mov 1, %edi
        0xb8,
        0x01,
        0x00,
        0x00,
        0x00, // mov write syscall, %eax
        0x0f,
        0x05, // syscall
        0xb8,
        0x3c,
        0x00,
        0x00,
        0x00, // mov exit syscall, %eax
        0x31,
        0xff, // xor %edi, %edi
        0x0f,
        0x05, // syscall
    ];
    let mut bytes = vec![0_u8; CODE_OFFSET];
    bytes[..4].copy_from_slice(b"\x7fELF");
    bytes[4] = 2; // ELFCLASS64
    bytes[5] = 1; // little-endian
    bytes[6] = 1; // current ELF version
    bytes[16..18].copy_from_slice(&2_u16.to_le_bytes()); // ET_EXEC
    bytes[18..20].copy_from_slice(&62_u16.to_le_bytes()); // EM_X86_64
    bytes[20..24].copy_from_slice(&1_u32.to_le_bytes());
    bytes[24..32].copy_from_slice(&(0x400000_u64 + CODE_OFFSET as u64).to_le_bytes());
    bytes[32..40].copy_from_slice(&(ELF_HEADER_SIZE as u64).to_le_bytes());
    bytes[52..54].copy_from_slice(&(ELF_HEADER_SIZE as u16).to_le_bytes());
    bytes[54..56].copy_from_slice(&(PROGRAM_HEADER_SIZE as u16).to_le_bytes());
    bytes[56..58].copy_from_slice(&1_u16.to_le_bytes());
    bytes[58..60].copy_from_slice(&64_u16.to_le_bytes());

    let program_header = ELF_HEADER_SIZE;
    bytes[program_header..program_header + 4].copy_from_slice(&1_u32.to_le_bytes()); // PT_LOAD
    bytes[program_header + 4..program_header + 8].copy_from_slice(&5_u32.to_le_bytes()); // R|X
    bytes[program_header + 16..program_header + 24].copy_from_slice(&0x400000_u64.to_le_bytes());
    bytes[program_header + 24..program_header + 32].copy_from_slice(&0x400000_u64.to_le_bytes());
    let file_size = CODE_OFFSET + code.len() + message.len();
    bytes[program_header + 32..program_header + 40]
        .copy_from_slice(&(file_size as u64).to_le_bytes());
    bytes[program_header + 40..program_header + 48]
        .copy_from_slice(&(file_size as u64).to_le_bytes());
    bytes[program_header + 48..program_header + 56].copy_from_slice(&0x1000_u64.to_le_bytes());

    bytes.extend_from_slice(&code);
    bytes.extend_from_slice(&message);
    bytes
}
