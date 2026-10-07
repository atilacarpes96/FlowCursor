// Gravador de PNG mínimo (sem compressão), para as imagens de conferência.
use std::path::Path;

fn crc32(dados: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &b in dados {
        crc ^= b as u32;
        for _ in 0..8 {
            crc = if crc & 1 != 0 { (crc >> 1) ^ 0xEDB8_8320 } else { crc >> 1 };
        }
    }
    !crc
}

fn adler32(dados: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in dados {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}

fn pedaco(saida: &mut Vec<u8>, tipo: &[u8; 4], dados: &[u8]) {
    saida.extend((dados.len() as u32).to_be_bytes());
    let ini = saida.len();
    saida.extend(tipo);
    saida.extend(dados);
    let crc = crc32(&saida[ini..]);
    saida.extend(crc.to_be_bytes());
}

/// `rgba`: w*h pixels, 4 bytes cada, sem pré-multiplicação.
pub fn salvar(caminho: &Path, w: usize, h: usize, rgba: &[u8]) -> std::io::Result<()> {
    std::fs::write(caminho, codificar(w, h, rgba))
}

/// Arquivo .ico com uma imagem PNG por tamanho (aceito desde o Windows Vista).
pub fn salvar_ico(caminho: &Path, imagens: &[(usize, Vec<u8>)]) -> std::io::Result<()> {
    let pngs: Vec<Vec<u8>> = imagens.iter().map(|(lado, rgba)| codificar(*lado, *lado, rgba)).collect();
    let mut saida = Vec::new();
    saida.extend(0u16.to_le_bytes());
    saida.extend(1u16.to_le_bytes());
    saida.extend((imagens.len() as u16).to_le_bytes());
    let mut deslocamento = 6 + 16 * imagens.len();
    for ((lado, _), png) in imagens.iter().zip(&pngs) {
        let l = if *lado >= 256 { 0u8 } else { *lado as u8 };
        saida.extend([l, l, 0, 0]);
        saida.extend(1u16.to_le_bytes());
        saida.extend(32u16.to_le_bytes());
        saida.extend((png.len() as u32).to_le_bytes());
        saida.extend((deslocamento as u32).to_le_bytes());
        deslocamento += png.len();
    }
    for png in &pngs {
        saida.extend(png);
    }
    std::fs::write(caminho, saida)
}

fn codificar(w: usize, h: usize, rgba: &[u8]) -> Vec<u8> {
    let mut cru = Vec::with_capacity((w * 4 + 1) * h);
    for y in 0..h {
        cru.push(0);
        cru.extend_from_slice(&rgba[y * w * 4..(y + 1) * w * 4]);
    }
    let mut z = vec![0x78, 0x01];
    let blocos: Vec<&[u8]> = cru.chunks(65535).collect();
    for (i, bloco) in blocos.iter().enumerate() {
        z.push((i + 1 == blocos.len()) as u8);
        let n = bloco.len() as u16;
        z.extend(n.to_le_bytes());
        z.extend((!n).to_le_bytes());
        z.extend_from_slice(bloco);
    }
    z.extend(adler32(&cru).to_be_bytes());
    let mut ihdr = Vec::new();
    ihdr.extend((w as u32).to_be_bytes());
    ihdr.extend((h as u32).to_be_bytes());
    ihdr.extend([8, 6, 0, 0, 0]);
    let mut saida = b"\x89PNG\r\n\x1a\n".to_vec();
    pedaco(&mut saida, b"IHDR", &ihdr);
    pedaco(&mut saida, b"IDAT", &z);
    pedaco(&mut saida, b"IEND", &[]);
    saida
}
