
use once_cell::sync::Lazy;
use crate::uvierror::{UVIError, UVIResult};
use std::iter;
use std::sync::Arc;

/* The default Huffman tables used by motion JPEG frames. When a motion JPEG
 * frame does not have DHT tables, we should use the huffman tables suggested by
 * the JPEG standard. Each of these tables represents a member of the JHUFF_TBLS
 * struct so we can just copy it to the according JHUFF_TBLS member. */
// DC table 0 
static MJPG_DC0_BITS:[u8;16] = [
  0x00, 0x01, 0x05, 0x01, 0x01, 0x01, 0x01, 0x01,
  0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00
];
static MJPG_DC0_HUFFVAL:[u8;12] = [
  0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
  0x08, 0x09, 0x0A, 0x0B
];

// DC table 1 
static MJPG_DC1_BITS:[u8;16] = [
  0x00, 0x03, 0x01, 0x01, 0x01, 0x01, 0x01, 0x01,
  0x01, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00
];
static MJPG_DC1_HUFFVAL:[u8;12] = [
  0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07,
  0x08, 0x09, 0x0A, 0x0B
];
  
// AC table 0
static MJPG_AC0_BITS:[u8;16] = [
  0x00, 0x02, 0x01, 0x03, 0x03, 0x02, 0x04, 0x03,
  0x05, 0x05, 0x04, 0x04, 0x00, 0x00, 0x01, 0x7D
];
static MJPG_AC0_HUFFVAL:[u8;162] = [
  0x01, 0x02, 0x03, 0x00, 0x04, 0x11, 0x05, 0x12,
  0x21, 0x31, 0x41, 0x06, 0x13, 0x51, 0x61, 0x07,
  0x22, 0x71, 0x14, 0x32, 0x81, 0x91, 0xA1, 0x08,
  0x23, 0x42, 0xB1, 0xC1, 0x15, 0x52, 0xD1, 0xF0,
  0x24, 0x33, 0x62, 0x72, 0x82, 0x09, 0x0A, 0x16,
  0x17, 0x18, 0x19, 0x1A, 0x25, 0x26, 0x27, 0x28,
  0x29, 0x2A, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39,
  0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48, 0x49,
  0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58, 0x59,
  0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68, 0x69,
  0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78, 0x79,
  0x7A, 0x83, 0x84, 0x85, 0x86, 0x87, 0x88, 0x89,
  0x8A, 0x92, 0x93, 0x94, 0x95, 0x96, 0x97, 0x98,
  0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7,
  0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4, 0xB5, 0xB6,
  0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3, 0xC4, 0xC5,
  0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2, 0xD3, 0xD4,
  0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA, 0xE1, 0xE2,
  0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9, 0xEA,
  0xF1, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
  0xF9, 0xFA
];

// AC table 1
static MJPG_AC1_BITS:[u8;16] = [
  0x00, 0x02, 0x01, 0x02, 0x04, 0x04, 0x03, 0x04,
  0x07, 0x05, 0x04, 0x04, 0x00, 0x01, 0x02, 0x77
];
static MJPG_AC1_HUFFVAL:[u8;162] = [
  0x00, 0x01, 0x02, 0x03, 0x11, 0x04, 0x05, 0x21,
  0x31, 0x06, 0x12, 0x41, 0x51, 0x07, 0x61, 0x71,
  0x13, 0x22, 0x32, 0x81, 0x08, 0x14, 0x42, 0x91,
  0xA1, 0xB1, 0xC1, 0x09, 0x23, 0x33, 0x52, 0xF0,
  0x15, 0x62, 0x72, 0xD1, 0x0A, 0x16, 0x24, 0x34,
  0xE1, 0x25, 0xF1, 0x17, 0x18, 0x19, 0x1A, 0x26,
  0x27, 0x28, 0x29, 0x2A, 0x35, 0x36, 0x37, 0x38,
  0x39, 0x3A, 0x43, 0x44, 0x45, 0x46, 0x47, 0x48,
  0x49, 0x4A, 0x53, 0x54, 0x55, 0x56, 0x57, 0x58,
  0x59, 0x5A, 0x63, 0x64, 0x65, 0x66, 0x67, 0x68,
  0x69, 0x6A, 0x73, 0x74, 0x75, 0x76, 0x77, 0x78,
  0x79, 0x7A, 0x82, 0x83, 0x84, 0x85, 0x86, 0x87,
  0x88, 0x89, 0x8A, 0x92, 0x93, 0x94, 0x95, 0x96,
  0x97, 0x98, 0x99, 0x9A, 0xA2, 0xA3, 0xA4, 0xA5,
  0xA6, 0xA7, 0xA8, 0xA9, 0xAA, 0xB2, 0xB3, 0xB4,
  0xB5, 0xB6, 0xB7, 0xB8, 0xB9, 0xBA, 0xC2, 0xC3,
  0xC4, 0xC5, 0xC6, 0xC7, 0xC8, 0xC9, 0xCA, 0xD2,
  0xD3, 0xD4, 0xD5, 0xD6, 0xD7, 0xD8, 0xD9, 0xDA,
  0xE2, 0xE3, 0xE4, 0xE5, 0xE6, 0xE7, 0xE8, 0xE9,
  0xEA, 0xF2, 0xF3, 0xF4, 0xF5, 0xF6, 0xF7, 0xF8,
  0xF9, 0xFA
];

const LUT_BITS: u8 = 8;

// Section C.2
fn derive_huffman_codes(bits: &[u8; 16]) -> UVIResult<(Vec<u16>, Vec<u8>)> {
    // Figure C.1
    let huffsize = bits.iter()
                       .enumerate()
                       .fold(Vec::new(), |mut acc, (i, &value)| {
                           acc.extend(iter::repeat((i + 1) as u8).take(value as usize));
                           acc
                       });

    // Figure C.2
    let mut huffcode = vec![0u16; huffsize.len()];
    let mut code_size = huffsize[0];
    let mut code = 0u32;

    for (i, &size) in huffsize.iter().enumerate() {
        while code_size < size {
            code <<= 1;
            code_size += 1;
        }

        if code >= (1u32 << size) {
            return Err(UVIError::HuffmanBadCodeLength);
        }

        huffcode[i] = code as u16;
        code += 1;
    }

    Ok((huffcode, huffsize))
}

// A Huffman Table class
#[derive(Debug, Clone)]
struct HuffmanTable {
    values: Vec<u8>,
    delta: [i32; 16],
    maxcode: [i32; 16],

    lut: [(u8, u8); 1 << LUT_BITS],
}
impl HuffmanTable {
    fn new(bits: &[u8; 16], values: &[u8]) -> UVIResult<HuffmanTable> {
        let (huffcode, huffsize) = derive_huffman_codes(bits)?;

        // Section F.2.2.3
        // Figure F.15
        // delta[i] is set to VALPTR(I) - MINCODE(I)
        let mut delta = [0i32; 16];
        let mut maxcode = [-1i32; 16];
        let mut j = 0;

        for i in 0 .. 16 {
            if bits[i] != 0 {
                delta[i] = j as i32 - huffcode[j] as i32;
                j += bits[i] as usize;
                maxcode[i] = huffcode[j - 1] as i32;
            }
        }

        // Build a lookup table for faster decoding.
        let mut lut = [(0u8, 0u8); 1 << LUT_BITS];

        for (i, &size) in huffsize.iter().enumerate().filter(|&(_, &size)| size <= LUT_BITS) {
            let bits_remaining = LUT_BITS - size;
            let start = (huffcode[i] << bits_remaining) as usize;

            let val = (values[i], size);
            for b in &mut lut[start..][..1 << bits_remaining] {
                *b = val;
            }
        }

        Ok(HuffmanTable{
            values: values.to_vec(),
            delta,
            maxcode,
            lut,
        })
    }

    // Section F.2.2.3
    // Figure F.16
    pub fn decode(&self, st: &mut Stream) -> UVIResult<u8> {
        let (value, size) = self.lut[st.peek_bits(LUT_BITS) as usize];

        if size > 0 {
            st.consume_bits(size);
            Ok(value)
        }
        else {
            let bits = st.peek_bits(16);

            for i in LUT_BITS .. 16 {
                let code = (bits >> (15 - i)) as i32;

                if code <= self.maxcode[i as usize] {
                    st.consume_bits(i + 1);

                    let index = (code + self.delta[i as usize]) as usize;
                    return Ok(self.values[index]);
                }
            }

            Err(UVIError::HuffmanDecodeError)
        }
    }
}

static HUFFMAN_TABLES_BASE:Lazy<[Arc<HuffmanTable>;0x20]> = Lazy::new(|| {
    let tbl00 = Arc::new(HuffmanTable::new(&MJPG_DC0_BITS, &MJPG_DC0_HUFFVAL).unwrap());
    let tbl01 = Arc::new(HuffmanTable::new(&MJPG_DC1_BITS, &MJPG_DC1_HUFFVAL).unwrap());
    let tbl10 = Arc::new(HuffmanTable::new(&MJPG_AC0_BITS, &MJPG_AC0_HUFFVAL).unwrap());
    let tbl11 = Arc::new(HuffmanTable::new(&MJPG_AC1_BITS, &MJPG_AC1_HUFFVAL).unwrap());
    [
        tbl00.clone(),tbl01.clone(), tbl00.clone(),tbl01.clone(),
        tbl00.clone(),tbl01.clone(), tbl00.clone(),tbl01.clone(),
        tbl00.clone(),tbl01.clone(), tbl00.clone(),tbl01.clone(),
        tbl00.clone(),tbl01.clone(), tbl00.clone(),tbl01.clone(),

        tbl10.clone(),tbl11.clone(), tbl10.clone(),tbl11.clone(), 
        tbl10.clone(),tbl11.clone(), tbl10.clone(),tbl11.clone(), 
        tbl10.clone(),tbl11.clone(), tbl10.clone(),tbl11.clone(), 
        tbl10.clone(),tbl11.clone(), tbl10.clone(),tbl11.clone(), 
    ]
});

struct Stream<'a> {
    data: &'a [u8],
    pos: usize,
    nbits_av: u8,
    bits_av: u64
}
impl<'a> Stream<'a> {
    fn new(data: &[u8]) -> Stream {
        Stream{
            data: &data,
            pos: 0,
            nbits_av: 0, bits_av: 0
        }
    }
    #[inline]
    fn get_extra_bits(&mut self) {
        let byte = match self.data.get(self.pos) {
            Some(b) => *b,
            None => 0
        };
        self.pos += 1;
        self.bits_av |= (byte as u64) << (56 - self.nbits_av);
        self.nbits_av += 8;
    }
    #[inline]
    fn peek_bits(&mut self, count: u8) -> u32 {
        assert!(count <= 32);
        while self.nbits_av < count {
            self.get_extra_bits();
        }
        ((self.bits_av >> (64 - count)) & ((1 << count) - 1)) as u32
    }
    #[inline]
    fn consume_bits(&mut self, count: u8) {
        assert!(count <= 32);
        while self.nbits_av < count {
            self.get_extra_bits();
        }
        self.bits_av <<= count as usize;
        self.nbits_av -= count;
    }
    fn get_extras(&self) -> i32 {
        (self.data.len() as i32)+((self.nbits_av>>3) as i32)-(self.pos as i32)
    }
}

fn build_matrix(huffman_tables:&[Arc<HuffmanTable>; 0x20], st: &mut Stream, 
    id_huffman_dc: u8, id_huffman_ac: u8) -> UVIResult<()> {
    let mut code = huffman_tables[(id_huffman_dc & 0xF) as usize].decode(st)?;
    st.consume_bits(code); // consume DC

    let mut l = 1;
    while l < 64 {
        code = huffman_tables[(0x10|(id_huffman_ac & 0xF)) as usize].decode(st)?;
        if code == 0 {
            break;
        }

        // The first part of the AC key_len
        // is the number of leading zeros
        l += code >> 4 + 1;
        st.consume_bits(code & 0x0F); // consume AC
    }
    Ok(())
}

#[derive(Debug)]
struct JpegComponent {
    id_component:u8,
    id_huffman_dc:u8, id_huffman_ac:u8,
    repeat:u8
}

pub fn get_good_jpeg(data: &[u8]) -> UVIResult<Vec<u8>> {
    let mut restart_interv: u16 = 0;
    let mut components: Vec<JpegComponent> = Vec::new();
    let mut huffman_tables:[Arc<HuffmanTable>; 0x20] = HUFFMAN_TABLES_BASE.clone();

    let datalen = data.len();
    if datalen < 32 {
        return Err(UVIError::BadJpegError);
    }
    //Check for valid JPEG image
    if data[0..4] != [0xFF, 0xD8, 0xFF, 0xE0] {
        return Err(UVIError::BadJpegError);
    }

    let mut blk: Vec<u8> = Vec::with_capacity(500000);
    let mut res: Vec<u8> = Vec::with_capacity(2000000);

    let mut pi = 0;
    while data[pi+1] != 0xD9 {
        // Section B.1.1.2
        // "Any marker may optionally be preceded by any number of fill bytes, which are bytes assigned code X’FF’."
        if data[pi+1] == 0xFF {
            continue;
        }
        blk.clear();
        let ps = pi;
        blk.push(data[pi]);
        pi += 1;
        loop {
            if pi+2 > datalen {
                return Err(UVIError::BadJpegError);
            }
            let el = data[pi];
            if el != 0xFF {
                blk.push(el);
                pi += 1;
            } else {
                match data[pi+1] {
                    0x00 => {
                        blk.push(0xFF);
                        pi += 2;
                    },
                    _ => {
                        break;
                    }
                }
            }
        }

        match blk[1] {
            0xC0 => {  //0xFFC0 is the "Start of frame" marker which contains the file size
                if blk.len() < 10 {
                    return Err(UVIError::BadJpegError);
                }
                /* The byte structure of the segment (in order):
                    - 2 bytes: length of the segment
                    - 1 byte: sample precision
                    - 1 byte: image height
                    - 1 byte: image width
                    - 1 byte: amount of color components
                    
                    - For each color component:
                        - 1 byte: ID of the component
                        - 4 bits: horizontal sample
                        - 4 bits: vertical sample
                        - 1 byte: ID of the quantization table used on the component */
                //The structure of the 0xFFC0 block is quite simple [0xFFC0][ushort length][uchar precision][ushort x][ushort y]
                let ncomponents = blk[9] as u8;
                if blk.len() < 10+(ncomponents as usize)*3 {
                    return Err(UVIError::BadJpegError);
                }
                components = Vec::new();
                for c in 0..ncomponents as usize {
                    let sampling = blk[11+c*3];
                    let horiz_sampling = sampling >> 4; let vert_sampling = sampling & 0xF;
                    let repeat = horiz_sampling*vert_sampling;
                    components.push(JpegComponent {
                        id_component: blk[10+c*3],
                        id_huffman_dc: 0, id_huffman_ac: 0,
                        repeat: repeat
                    });
                }
                res.extend_from_slice(&data[ps..pi]);
            },
            0xC4 => {
                if blk.len() < 30 {
                    return Err(UVIError::BadJpegError);
                }
                huffman_tables[(blk[5] & 0x1F) as usize] = 
                    Arc::new(HuffmanTable::new(&blk[6..6+16].try_into().unwrap(), &blk[6+16..])?);
            },
            0xDD => {
                if blk.len() >= 6  {
                    restart_interv = u16::from_be_bytes(blk[4..6].try_into().unwrap());
                }
                res.extend_from_slice(&data[ps..pi]);
            },
            0xDA => {
                /* The structure of the Start of Scan header is (in order):
                    - 2 bytes: length of the segment
                    - 1 byte: amount of color components in the current scan
                    - For each color component in the scan:
                        - 1 byte: ID of the component
                        - 4 bits: ID of the Huffman table for DC values of the component
                        - 4 bits: ID of the Huffman table for AC values of the component
                    - 1 byte: Start of the spectral selection
                    - 1 byte: End of the spectral selection
                    - 4 bits: Successive approximation (high)
                    - 4 bits: Successive approximation (low) */
                let prev = u16::from_be_bytes(blk[2..4].try_into().unwrap()) as usize;
                let ncomponents = blk[4];
                for i in 0..ncomponents as usize {
                    for pcomponent in &mut components {
                        if blk[5+i*2] == pcomponent.id_component {
                            let ids = blk[6+i*2];
                            pcomponent.id_huffman_dc = ids>>4;
                            pcomponent.id_huffman_ac = ids & 0xF;
                        }
                    }
                }
                //println!("components {:?}", components);
                let mut pi2 = pi;
                if restart_interv > 0 && blk[blk.len()-1] == 0xFF {
                    let mut st = Stream::new(&blk[(2+prev)..]);
                    let restart_count = restart_interv;
                    for _n_mcu in (0..restart_count).step_by(1) {
                        //println!("n_mcu {}", n_mcu);
                        for component in &components {
                            for _r in 0..component.repeat {
                                build_matrix(&huffman_tables, &mut st,
                                    component.id_huffman_dc, component.id_huffman_ac)?;
                            }
                        }
                    }
                    if st.get_extras() != 0 {
                        pi2 -= 2;
                    }
                }
                res.extend_from_slice(&data[ps..pi2]);
            },
            0xD0|0xD1|0xD2|0xD3|0xD4|0xD5|0xD6|0xD7 => {
                let mut pi2 = pi;
                if blk[blk.len()-1] == 0xFF {
                    let mut st = Stream::new(&blk[2..]);
                    for _n_mcu in (0..restart_interv).step_by(1) {
                        //println!("n_mcu {}", n_mcu);
                        for component in &components {
                            for _r in 0..component.repeat {
                                build_matrix(&huffman_tables, &mut st,
                                    component.id_huffman_dc, component.id_huffman_ac)?;
                            }
                        }
                    }
                    if st.get_extras() != 0 {
                        pi2 -= 2;
                    }
                }
                res.extend_from_slice(&data[ps..pi2]);
            },
            _ => {
                res.extend_from_slice(&data[ps..pi]);
            }
        }
    }
    res.push(0xFF);
    res.push(0xD9);
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Read;
    //use std::io::Write;
    
    #[test]
    fn compare_jpeg_fix_for_mjpeg() {
        let mut file = File::open("test_ref/prob.jpg").unwrap();
        let mut base = Vec::new();
        file.read_to_end(&mut base).unwrap();

        let mut file = File::open("test_ref/prob_res.jpg").unwrap();
        let mut good_res = Vec::new();
        file.read_to_end(&mut good_res).unwrap();

        let r = get_good_jpeg(&base).unwrap();

        //let mut file = File::create("/tmp/good.jpg").unwrap();
        //file.write_all(&r).unwrap();

        assert_eq!(good_res.len(), r.len());

        let mut not_matching = 0;
        for i in 0..r.len() {
            if good_res[i] != r[i] { not_matching += 1; }
        }
        assert_eq!(not_matching, 0);
    }
}