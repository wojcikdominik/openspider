//! Einziges Testbild der Probe: 32 × 32 Pixel, einfarbig rot, PNG (RGB, 8 Bit), im Code erzeugt (keine Binärdatei
//! im Repository). Deflate ohne Kompression (ein „stored“-Block), daher ohne Abhängigkeit.

/// Kantenlänge in Pixeln.
pub const SIZE: u32 = 32;
/// Farbe (R, G, B).
pub const COLOUR: [u8; 3] = [220, 20, 20];

/// CRC-32 (IEEE, wie PNG).
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in bytes {
        crc ^= u32::from(b);
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xEDB8_8320 & mask);
        }
    }
    !crc
}

/// Adler-32 (zlib-Prüfsumme).
pub fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in bytes {
        a = (a + u32::from(x)) % 65_521;
        b = (b + a) % 65_521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut crc_input = Vec::with_capacity(4 + data.len());
    crc_input.extend_from_slice(kind);
    crc_input.extend_from_slice(data);
    out.extend_from_slice(&crc_input);
    out.extend_from_slice(&crc32(&crc_input).to_be_bytes());
}

/// Die PNG-Bytes.
pub fn test_png() -> Vec<u8> {
    // Je Zeile: Filterbyte 0 („None“), dann SIZE Pixel.
    let mut row = vec![0u8];
    for _ in 0..SIZE {
        row.extend_from_slice(&COLOUR);
    }
    let raw: Vec<u8> = row.repeat(SIZE as usize);
    let len = raw.len() as u16; // 32 × 97 = 3.104 Byte, passt in einen stored-Block
    let mut zlib = vec![0x78, 0x01, 0x01];
    zlib.extend_from_slice(&len.to_le_bytes());
    zlib.extend_from_slice(&(!len).to_le_bytes());
    zlib.extend_from_slice(&raw);
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&SIZE.to_be_bytes());
    ihdr.extend_from_slice(&SIZE.to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &zlib);
    chunk(&mut out, b"IEND", &[]);
    out
}
