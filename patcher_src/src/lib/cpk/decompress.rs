pub fn crilayla_decompress(data: Vec<u8>) -> Vec<u8> {
    let mut buff = [0u8; 4];
    buff.copy_from_slice(&data[8..0xc]);
    let size = u32::from_le_bytes(buff) as usize;
    buff.copy_from_slice(&data[0xc..0x10]);
    let header = u32::from_le_bytes(buff) as usize;

    let mut res = vec![0; size+0x100];
    res[0..0x100].copy_from_slice(&data[header + 0x10..header + 0x110]);

    let input_end = data.len() - 0x100 - 1;
    let output_end = 0x100 + size - 1;

    let mut off = input_end;
    let mut curr = 0;
    let mut rem = 0;

    let vle_lens = vec![2, 3, 5, 8];
    let mut len = 0;

    macro_rules! get_next_bits {
        ($count:expr) => {{
            get_next_bits(&data, &mut off, &mut curr, &mut rem, $count)
            // let mut bits = 0;
            // let mut i = 0;
            // while i < $count {
            //     if rem == 0 {
            //         curr = data[off];
            //         off -= 1;
            //         rem = 8;
            //     }
            //     let to_copy = rem.min($count - i);
            //     bits <<= to_copy;
            //     bits |= (curr >> (rem - to_copy)) & ((1 << to_copy) - 1);
            //     rem -= to_copy;
            //     i += to_copy;
            // }
            // bits
        }};
    }

    while len < size {
        // let bit;
        let bit = get_next_bits!(1);
        // (bit, off, curr, rem) = get_next_bits(&data, off, curr, rem, 1);
        if bit == 1 {
            let mut backref_off =  get_next_bits!(13) + (output_end - len + 3);
            let mut backref_len = 3;
            for vle_level in vle_lens.iter() {
                let level = get_next_bits!(*vle_level);
                // (level, off, curr, rem) = get_next_bits(&data, off, curr, rem, vle_level);
                backref_len += level;
                if level != ((1 << vle_level) - 1) {
                    break;
                }
            }
            if backref_len == 3 + 3 + 7 + 31 + 255 {
                loop {
                    let level = get_next_bits!(8);
                    // (level, off, curr, rem) = get_next_bits(&data, off, curr, rem, 8);
                    backref_len += level;
                    if level != 255 {
                        break;
                    }
                }
            }
            for _ in 0..backref_len {
                res[output_end - len] = res[backref_off];
                backref_off -= 1;
                len += 1;
            }
        } else {
            let byte = get_next_bits!(8);
            // (byte, off, curr, rem) = get_next_bits(&data, off, curr, rem, 8);

            res[output_end - len] = byte as u8;
            len += 1;
        }
    }
    res
}
// public byte[] DecompressLegacyCRI(byte[] input, int USize)
// {
//     byte[] result;// = new byte[USize];

//     MemoryStream ms = new MemoryStream(input);
//     EndianReader br = new EndianReader(ms, true);

//     br.BaseStream.Seek(8, SeekOrigin.Begin); // Skip CRILAYLA
//     int uncompressed_size = br.ReadInt32();
//     int uncompressed_header_offset = br.ReadInt32();

//     result = new byte[uncompressed_size + 0x100];

//     // do some error checks here.........

//     // copy uncompressed 0x100 header to start of file
//     Array.Copy(input, uncompressed_header_offset + 0x10, result, 0, 0x100);

//     int input_end = input.Length - 0x100 - 1;
//     int input_offset = input_end;
//     int output_end = 0x100 + uncompressed_size - 1;
//     byte bit_pool = 0;
//     int bits_left = 0, bytes_output = 0;
//     int[] vle_lens = new int[4] { 2, 3, 5, 8 };

//     while (bytes_output < uncompressed_size)
//     {
//         if (get_next_bits(input, ref input_offset, ref  bit_pool, ref bits_left, 1) > 0)
//         {
//             int backreference_offset = output_end - bytes_output + get_next_bits(input, ref input_offset, ref  bit_pool, ref bits_left, 13) + 3;
//             int backreference_length = 3;
//             int vle_level;

//             for (vle_level = 0; vle_level < vle_lens.Length; vle_level++)
//             {
//                 int this_level = get_next_bits(input, ref input_offset, ref  bit_pool, ref bits_left, vle_lens[vle_level]);
//                 backreference_length += this_level;
//                 if (this_level != ((1 << vle_lens[vle_level]) - 1)) break;
//             }

//             if (vle_level == vle_lens.Length)
//             {
//                 int this_level;
//                 do
//                 {
//                     this_level = get_next_bits(input, ref input_offset, ref  bit_pool, ref bits_left, 8);
//                     backreference_length += this_level;
//                 } while (this_level == 255);
//             }

//             for (int i = 0; i < backreference_length; i++)
//             {
//                 result[output_end - bytes_output] = result[backreference_offset--];
//                 bytes_output++;
//             }
//         }
//         else
//         {
//             // verbatim byte
//             result[output_end - bytes_output] = (byte)get_next_bits(input, ref input_offset, ref  bit_pool, ref bits_left, 8);
//             bytes_output++;
//         }
//     }

//     br.Close();
//     ms.Close();

//     return result;
// }

fn get_next_bits(
    input: &Vec<u8>,
    offset: &mut usize,
    curr: &mut u8,
    rem: &mut usize,
    count: usize,
) -> usize {
    let mut bits = 0;
    let mut i = 0;
    while i < count {
        if *rem == 0 {
            *curr = input[*offset];
            *offset -= 1;
            *rem = 8;
        }
        let to_copy = (*rem).min(count - i);
        bits <<= to_copy;
        bits |= (*curr >> (*rem - to_copy)) as usize & ((1 << to_copy) - 1);
        *rem -= to_copy;
        i += to_copy;
    }
    bits
    // (bits, offset, curr, rem)
}
// private ushort get_next_bits(byte[] input, ref int offset_p, ref byte bit_pool_p, ref int bits_left_p, int bit_count)
// {
//     ushort out_bits = 0;
//     int num_bits_produced = 0;
//     int bits_this_round;

//     while (num_bits_produced < bit_count)
//     {
//         if (bits_left_p == 0)
//         {
//             bit_pool_p = input[offset_p];
//             bits_left_p = 8;
//             offset_p--;
//         }

//         if (bits_left_p > (bit_count - num_bits_produced))
//             bits_this_round = bit_count - num_bits_produced;
//         else
//             bits_this_round = bits_left_p;

//         out_bits <<= bits_this_round;

//         out_bits |= (ushort)((ushort)(bit_pool_p >> (bits_left_p - bits_this_round)) & ((1 << bits_this_round) - 1));

//         bits_left_p -= bits_this_round;
//         num_bits_produced += bits_this_round;
//     }

//     return out_bits;
// }
