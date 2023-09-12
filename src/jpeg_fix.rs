
use crate::uvierror::{UVIError, UVIResult};

pub fn get_good_jpeg(data: &[u8]) -> UVIResult<Vec<u8>> {
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
            0xDA|0xD0|0xD1|0xD2|0xD3|0xD4|0xD5|0xD6|0xD7 => {
                if blk[blk.len()-1] == 0xFF {
                    // not always right
                    blk.pop();
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
    use std::io::Write;
    
    #[test]
    fn compare_jpeg_fix_for_mjpeg() {
        let mut file = File::open("test_ref/prob.jpg").unwrap();
        let mut base = Vec::new();
        file.read_to_end(&mut base).unwrap();

        let mut file = File::open("test_ref/prob_res.jpg").unwrap();
        let mut good_res = Vec::new();
        file.read_to_end(&mut good_res).unwrap();

        let r = get_good_jpeg(&base).unwrap();

        let mut file = File::create("/tmp/good.jpg").unwrap();
        file.write_all(&r).unwrap();

        assert_eq!(good_res.len(), r.len());

        let mut not_matching = 0;
        for i in 0..r.len() {
            if good_res[i] != r[i] { not_matching += 1; }
        }
        assert_eq!(not_matching, 0);
    }
}