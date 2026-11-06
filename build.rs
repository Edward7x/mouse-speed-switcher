use std::{env, fs, path::PathBuf};

fn word(out: &mut Vec<u8>, value: u16) {
    out.extend(value.to_le_bytes());
}
fn dword(out: &mut Vec<u8>, value: u32) {
    out.extend(value.to_le_bytes());
}
fn resource(out: &mut Vec<u8>, kind: u16, id: u16, data: &[u8]) {
    dword(out, data.len() as u32);
    dword(out, 32);
    for value in [0xffff, kind, 0xffff, id] {
        word(out, value);
    }
    dword(out, 0);
    word(out, 0x1030);
    word(out, 0);
    dword(out, 0);
    dword(out, 0);
    out.extend(data);
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
}
fn version_block(key: &str, value: &[u8], text: bool, children: &[u8]) -> Vec<u8> {
    let mut out = vec![0; 6];
    for ch in key.encode_utf16().chain(Some(0)) {
        word(&mut out, ch);
    }
    while !out.len().is_multiple_of(4) {
        out.push(0);
    }
    out.extend(value);
    if !children.is_empty() {
        while !out.len().is_multiple_of(4) {
            out.push(0);
        }
        out.extend(children);
    }
    let len = out.len() as u16;
    out[..2].copy_from_slice(&len.to_le_bytes());
    out[2..4]
        .copy_from_slice(&(if text { value.len() / 2 } else { value.len() } as u16).to_le_bytes());
    out[4..6].copy_from_slice(&u16::from(text).to_le_bytes());
    out
}
fn version_resource() -> Vec<u8> {
    let version = env::var("CARGO_PKG_VERSION").unwrap();
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap()).collect();
    let mut fixed = Vec::new();
    for value in [
        0xfeef04bd,
        0x10000,
        (parts[0] << 16) | parts[1],
        parts[2] << 16,
        (parts[0] << 16) | parts[1],
        parts[2] << 16,
        0x3f,
        0,
        0x40004,
        1,
        0,
        0,
        0,
    ] {
        dword(&mut fixed, value);
    }
    let mut strings = Vec::new();
    for (key, value) in [
        ("ProductName", "Mouse Speed Switcher"),
        ("FileDescription", "Mouse Speed Switcher"),
        ("FileVersion", version.as_str()),
        ("ProductVersion", version.as_str()),
    ] {
        while !strings.len().is_multiple_of(4) {
            strings.push(0);
        }
        let bytes: Vec<u8> = value
            .encode_utf16()
            .chain(Some(0))
            .flat_map(u16::to_le_bytes)
            .collect();
        strings.extend(version_block(key, &bytes, true, &[]));
    }
    let table = version_block("040904B0", &[], true, &strings);
    let mut children = version_block("StringFileInfo", &[], true, &table);
    while !children.len().is_multiple_of(4) {
        children.push(0);
    }
    let translation = version_block("Translation", &[9, 4, 0xb0, 4], false, &[]);
    children.extend(version_block("VarFileInfo", &[], true, &translation));
    version_block("VS_VERSION_INFO", &fixed, false, &children)
}
fn main() {
    println!("cargo:rerun-if-changed=assets/app-icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    // Emit standard Win32 .res records directly, avoiding an SDK resource compiler.
    let ico = fs::read("assets/app-icon.ico").expect("read application icon");
    let count = u16::from_le_bytes([ico[4], ico[5]]);
    let mut res = Vec::new();
    resource(&mut res, 0, 0, &[]);
    let mut group = ico[..6].to_vec();
    for i in 0..count as usize {
        let entry = &ico[6 + i * 16..22 + i * 16];
        let size = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().unwrap()) as usize;
        resource(&mut res, 3, i as u16 + 1, &ico[offset..offset + size]);
        group.extend(&entry[..12]);
        word(&mut group, i as u16 + 1);
    }
    resource(&mut res, 14, 1, &group);
    resource(&mut res, 16, 1, &version_resource());
    let path = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("app-icon.res");
    fs::write(&path, res).expect("write application icon resource");
    println!("cargo:rustc-link-arg={}", path.display());
}
