fn main() {
    let mut ifdp = inverse_fdp::Ifdp::new();
    // Insert below

    // Taken from fuzz input qa-assets/fuzz_corpora/process_message/f5253cc5b2b243e59eb03205f7321674a8a6e326
    ifdp.push_bytes(&[
        0x74, 0x78, 0x0, 0x66, 0xe6, 0x4, 0x0, 0x20, 0x0, 0x70, 0x2b, 0x1,
    ]); // (len=12)
    ifdp.push_integral_in_range(1095216660480i64, 0i64, 9223372036854775807i64);
    ifdp.push_integral_in_range(4294967294u32, 4294967294u32, 4294967295u32);
    ifdp.push_integral_in_range(0u8, 0u8, 255u8);
    ifdp.push_integral_in_range(0u64, 0u64, 5u64);
    ifdp.push_bytes(&[0xc0, 0x0, 0x42, 0x0]); // (len=4)
    ifdp.push_integral_in_range(26992u16, 0u16, 65535u16);
    ifdp.push_integral_in_range(97u8, 0u8, 255u8);
    ifdp.push_integral_in_range(1u64, 0u64, 6u64);
    ifdp.push_integral_in_range(16449536u32, 0u32, 4294967295u32);
    ifdp.push_integral_in_range(1127013566931252u64, 0u64, 18446744073709551615u64);
    ifdp.push_integral_in_range(3711527921903402496u64, 0u64, 18446744073709551615u64);
    ifdp.push_integral_in_range(0u64, 0u64, 5u64);
    ifdp.push_bytes(&[0xcc, 0xf6, 0x2, 0x0]); // (len=4)
    ifdp.push_integral_in_range(3111u16, 0u16, 65535u16);
    ifdp.push_integral_in_range(217u8, 0u8, 255u8);
    ifdp.push_integral_in_range(2u64, 0u64, 6u64);
    ifdp.push_integral_in_range(1900544u32, 0u32, 4294967295u32);
    ifdp.push_str_u8(&[0x8, 0x0]); // (len=2), Limit: 64 (When the limit is equal to the len, it may actually be unlimited in the format)
    ifdp.push_integral_in_range(2u64, 0u64, 6u64);
    ifdp.push_integral_in_range(18374687579274834944u64, 0u64, 18446744073709551615u64);
    ifdp.push_integral_in_range(59u8, 0u8, 255u8);
    ifdp.push_integral_in_range(4u64, 0u64, 9u64);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(3u64, 0u64, 6u64);
    ifdp.push_integral_in_range(251u8, 0u8, 255u8);
    ifdp.push_integral_in_range(1u64, 0u64, 6u64);
    ifdp.push_integral_in_range(50560056i32, 31800i32, 2147483647i32);
    ifdp.push_integral_in_range(124u8, 0u8, 255u8);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(7u8, 0u8, 255u8);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(255u8, 0u8, 255u8);
    ifdp.push_integral_in_range(135u8, 0u8, 255u8);
    ifdp.push_integral_in_range(172u8, 0u8, 255u8);
    ifdp.push_integral_in_range(1i64, -1i64, 9i64);
    ifdp.push_integral_in_range(2615519419i64, 946684801i64, 4133980799i64);
    ifdp.push_str_u8(&[0x0, 0x0]); // (len=2), Limit: 4000000 (When the limit is equal to the len, it may actually be unlimited in the format)

    //                 ^^^^^^^^
    // Those two bytes are the TX payload (untested)
    // A hex payload can be copy-pasted into this.

    // Insert above
    let buffer = ifdp.retrieve_bytes();
    use std::fs::File;
    use std::io::Write;
    File::create("/tmp/ifdp.out")
        .unwrap()
        .write_all(&buffer)
        .unwrap();
}
