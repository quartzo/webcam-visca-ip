
use std::collections::HashMap;
use once_cell::sync::Lazy;
use crate::uvierror::{UVIError, UVIResult};

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

// A Huffman Table class
#[derive(Debug, Clone)]
struct HuffmanTable {
    root: HuffmanTableTreeEl,
    /*elements: Vec<u8>,*/
}
#[derive(Debug, Clone)]
enum HuffmanTableTreeEl {
    Element(u8),
    List(Vec<HuffmanTableTreeEl>)
}
impl HuffmanTable {
    fn bits_from_lengths(root: &mut HuffmanTableTreeEl, element: u8, pos: u8) -> bool {
        match root {
            HuffmanTableTreeEl::List(rootl) => {
                if pos == 0 {
                    if rootl.len() < 2 {
                        rootl.push(HuffmanTableTreeEl::Element(element));
                        return true;
                    }
                    return false;
                }
                for i in 0..2 {
                    if rootl.len() == i {
                        rootl.push(HuffmanTableTreeEl::List(Vec::new()));
                    }
                    if HuffmanTable::bits_from_lengths(&mut rootl[i], element, pos - 1) == true {
                        return true;
                    }
                }
                return false;
            },
            _ => {}
        }
        false
    }
    fn get_huffman_bits(lengths: &[u8], elements: &[u8]) -> HuffmanTable {
        let mut ht = HuffmanTable{
            root: HuffmanTableTreeEl::List(Vec::new()),
            /*elements: elements.to_vec()*/
        };
        let mut ii = 0;
        for i in 0..lengths.len() {
            for _j in 0..lengths[i] {
                HuffmanTable::bits_from_lengths(&mut ht.root, elements[ii], i as u8);
                ii += 1;
            }
        }
        //println!("r: {:?}",ht);
        ht
    }
    fn find(&self, st: &mut Stream) -> u8 {
        let mut r = &self.root;
        match r {
            HuffmanTableTreeEl::List(rl) => {
                if rl.len() == 1 {
                    return match rl[0] {
                        HuffmanTableTreeEl::List(_) => 0,
                        HuffmanTableTreeEl::Element(el) => el
                    };
                }
            },
            HuffmanTableTreeEl::Element(_) => {
                return 0;
            }
        }
        let mut bits: u8 = 0;
        loop {
            match r {
                HuffmanTableTreeEl::List(rl) => {
                    let bit = st.get_bit();
                    bits = bits << 1 | bit;
                    r = &rl[bit as usize];
                },
                HuffmanTableTreeEl::Element(el) => {
                    //println!("find {:02x}", bits);
                    return *el;
                }
            }
        }
    }
    fn get_code(&self, st: &mut Stream) -> u8 {
        loop {
            let res = self.find(st);
            if res == 0 {
                return 0;
            } /*else if res != -1 { */
                return res;
            /*}*/
        }
    }
}

static HUFFMAN_TABLES_BASE:Lazy<HashMap<u8,HuffmanTable>> = Lazy::new(|| {
    let mut tbl = HashMap::new();
    tbl.insert(0, HuffmanTable::get_huffman_bits(&MJPG_DC0_BITS, &MJPG_DC0_HUFFVAL));
    tbl.insert(1, HuffmanTable::get_huffman_bits(&MJPG_DC1_BITS, &MJPG_DC1_HUFFVAL));
    tbl.insert(0x10 | 0, HuffmanTable::get_huffman_bits(&MJPG_AC0_BITS, &MJPG_AC0_HUFFVAL));
    tbl.insert(0x10 | 1, HuffmanTable::get_huffman_bits(&MJPG_AC1_BITS, &MJPG_AC1_HUFFVAL));
    tbl
});

// A bit stream class with convenience methods
struct Stream {
    data: Vec<u8>,
    pos: usize
}
impl Stream {
    fn new(data: &[u8]) -> Stream {
        Stream{
            data: data.to_vec(),
            pos: 0
        }
    }
    fn get_bit(&mut self) -> u8 {
        let bpos = self.pos >> 3;
        let b = if bpos < self.data.len() {
            self.data[bpos]
        } else {
            0
        };
        let s = 7 - (self.pos & 0x7);
        self.pos += 1;
        let r = (b >> s) & 1;
        return r;
    }
    fn get_bit_n(&mut self, l: u8) -> u32 {
        let mut val: u32 = 0;
        for _ in 0..l {
            val = val<<1 | (self.get_bit() as u32);
        }
        //println!("get_bit_n {:02x}", val);
        val
    }
    fn get_extras(&self) -> i32 {
        //println!("len {} pos {}", self.data.len(), self.pos);
        (self.data.len() as i32)-((self.pos+7) >> 3) as i32
    }
}

fn build_matrix(huffman_tables:&HashMap<u8,HuffmanTable>, st: &mut Stream, 
    id_huffman_dc: u8, id_huffman_ac: u8) {
    let mut code = huffman_tables[&(0 + id_huffman_dc)].get_code(st);
    let mut _bits = st.get_bit_n(code);

    let mut l = 1;
    while l < 64 {
        code = huffman_tables[&(16 + id_huffman_ac)].get_code(st);
        if code == 0 {
            break;
        }

        // The first part of the AC key_len
        // is the number of leading zeros
        if code > 15 {
            l += code >> 4;
            code = code & 0x0F;
        }

        _bits = st.get_bit_n(code);

        if l < 64 {
            l += 1;
        }
    }
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
    let mut huffman_tables:HashMap<u8,HuffmanTable> = HUFFMAN_TABLES_BASE.clone();

    let datalen = data.len();
    if datalen < 32 {
        return Err(UVIError::BadJpegError);
    }
    //Check for valid JPEG image
    if data[0..4] != [0xFF, 0xD8, 0xFF, 0xE0] {
        return Err(UVIError::BadJpegError);
    }

    let mut res: Vec<u8> = Vec::new();

    let mut pi = 0;
    while data[pi+1] != 0xD9 {
        let mut blk:Vec<u8> = Vec::new();
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
            },
            0xC4 => {
                if blk.len() < 30 {
                    return Err(UVIError::BadJpegError);
                }
                huffman_tables.insert(blk[5], HuffmanTable::get_huffman_bits(&blk[6..6+16], &blk[6+16..]));
            },
            0xDD => {
                if blk.len() >= 6  {
                    restart_interv = u16::from_be_bytes(blk[4..6].try_into().unwrap());
                }
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
                if restart_interv > 0 && blk[blk.len()-1] == 0xFF {
                    let mut st = Stream::new(&blk[(2+prev)..]);
                    let restart_count = restart_interv;
                    for _n_mcu in (0..restart_count).step_by(1) {
                        //println!("n_mcu {}", n_mcu);
                        for component in &components {
                            for _r in 0..component.repeat {
                                build_matrix(&huffman_tables, &mut st,
                                    component.id_huffman_dc, component.id_huffman_ac);
                            }
                        }
                    }
                    //if st.get_extras() != 0 {
                    //    println!("extras {}", st.get_extras());
                    //}
                    for _i in 0..st.get_extras() {
                        blk.pop();
                    }
                }
            },
            0xD0|0xD1|0xD2|0xD3|0xD4|0xD5|0xD6|0xD7 => {
                if blk[blk.len()-1] == 0xFF {
                    let mut st = Stream::new(&blk[2..]);
                    for _n_mcu in (0..restart_interv).step_by(1) {
                        //println!("n_mcu {}", n_mcu);
                        for component in &components {
                            for _r in 0..component.repeat {
                                build_matrix(&huffman_tables, &mut st,
                                    component.id_huffman_dc, component.id_huffman_ac);
                            }
                        }
                    }
                    //if st.get_extras() != 0 {
                    //    println!("extras {}", st.get_extras());
                    //}
                    for _i in 0..st.get_extras() {
                        blk.pop();
                    }
                }
            },
            _ => {}
        }

        res.push(0xFF);
        for el in &blk[1..] {
            if *el == 0xFF {
                res.push(0xFF); res.push(0x00);
            } else {
                res.push(*el);
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