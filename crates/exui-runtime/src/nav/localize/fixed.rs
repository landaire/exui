//! `%f` of the system `_snwprintf` that `0x921568F0` formats float property
//! values with (xboxkrnl ordinal 0x13D; 0x8014CAB0 in kernel 2.0.16547).
//! The double becomes a 12-byte extended value (0x8014FB90), 17 digits
//! are generated with extended arithmetic (0x8014FD18), and the digit
//! string is rounded half up to the precision (0x8014FAB8) and laid out
//! (0x8014F6A0).

use alloc::vec::Vec;

/// Digits generated before the last one rounds them (0x8014FC70).
const SIGNIFICANT_DIGITS: usize = 17;

const LOW_96: u128 = (1 << 96) - 1;
const MANTISSA: u128 = (1 << 80) - 1;

/// 1/10 as the stack constant of 0x8014FD18.
const TENTH: Ld12 = Ld12(0x3ffb_cccc_cccc_cccc_cccc_cccc);

/// Powers of ten 10^1 to 10^7, 10^8 to 10^56 and so on, at 0x8005A908.
const POSITIVE: [u128; 28] = [
    0x4002a0000000000000000000,
    0x4005c8000000000000000000,
    0x4008fa000000000000000000,
    0x400c9c400000000000000000,
    0x400fc3500000000000000000,
    0x4012f4240000000000000000,
    0x401698968000000000000000,
    0x4019bebc2000000000000000,
    0x40348e1bc9bf040000000000,
    0x404ed3c21bcecceda1000000,
    0x40699dc5ada82b70b59ef020,
    0x4083eb194f8e1ae525fd5dd0,
    0x409eaf298d050e4395d79671,
    0x40b982818f1281ed44a0bff9,
    0x40d3c2781f49ffcfa6d53cbf,
    0x41a893ba47c980e98ce0c66f,
    0x427ce070f78d3927556b85bc,
    0x4351aa7eebfb9df9de8eddbc,
    0x442681842f29f2cce376e6a1,
    0x44fac4c5e310aef8aa171028,
    0x45cf957a4ae1ebf7f3d4a7eb,
    0x46a3e319a0aea60e91c7cc65,
    0x4d48c976758681750c17650d,
    0x53edb2b8353b3993a7e44258,
    0x5a929e8b3b5dc53d5de5a74d,
    0x61378ca554c020a1f0a65dff,
    0x67dbf9895d25d88b5a8bfdd1,
    0x6e80dd5dc8a2bf27f3f895aa,
];

/// Powers of ten 10^-1 and down, at 0x8005AA68.
const NEGATIVE: [u128; 28] = [
    0x3ffbcccccccccccccccdcccd,
    0x3ff8a3d70a3d70a3d70a3d71,
    0x3ff583126e978d4fdf3b645a,
    0x3ff1d1b71758e219652cd3c3,
    0x3feea7c5ac471b4784230fd0,
    0x3feb8637bd05af6c69b6a640,
    0x3fe7d6bf94d5e57a42bc3d33,
    0x3fe4abcc77118461cefdfdc2,
    0x3fc9e69594bec44de15b4c2f,
    0x3faf9abe14cd44753b53c492,
    0x3f94cfb11ead453994ba67de,
    0x3f7a8b61313bbabce2c62324,
    0x3f5fbb127c53b17ec1595561,
    0x3f44fb158592be068d2feed7,
    0x3f2aa87fea27a539e9a53f24,
    0x3e55ddd0467c64bce4a1ac7d,
    0x3d8191ff83775423cc067b63,
    0x3cacc0314325637a193afa91,
    0x3bd7fd00b897478238d18921,
    0x3b03a686e3e8b11b085888dc,
    0x3a2edb377599b607424584c6,
    0x395a9049ee32db23d21c7133,
    0x32b5a2a682a5da57c0be87a6,
    0x2c10b759449f52a711b268e2,
    0x256bceae534f34362de44925,
    0x1ec6e8fb7dc2dec0a404598f,
    0x18228350bf3c91575a88e79e,
    0x117d9406af8f83fd62654b4e,
];

/// The kernel's 12-byte extended value: sign and exponent in bits 80..95
/// above an 80-bit mantissa with an explicit integer bit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ld12(u128);

impl Ld12 {
    fn exponent(self) -> u16 {
        (self.0 >> 80) as u16
    }

    fn word(self, index: usize) -> u16 {
        (self.0 >> (16 * index)) as u16
    }

    fn infinity(sign: u16) -> Self {
        Self(((u128::from(0x7FFF | sign) << 16) | 0x8000) << 64)
    }
}

/// The 17 digits of a value: `0.digits * 10^exponent`.
struct Digits {
    negative: bool,
    exponent: i32,
    digits: Vec<u8>,
}

/// `value` as `%.<precision>f` writes it.
pub(super) fn format_fixed(value: f64, precision: usize) -> Vec<u8> {
    let Digits {
        negative,
        mut exponent,
        digits,
    } = digits(value);

    // 0x8014FAB8: a leading zero takes the carry of the rounding.
    let wanted = precision as i64 + i64::from(exponent);
    let mut text = Vec::new();
    text.push(b'0');
    let mut next = 0;
    for _ in 0..wanted.max(0) {
        match digits.get(next) {
            Some(&digit) => {
                text.push(digit);
                next += 1;
            }
            None => text.push(b'0'),
        }
    }
    if wanted >= 0 && digits.get(next).is_some_and(|&digit| digit >= b'5') {
        let mut index = text.len() - 1;
        while text[index] == b'9' {
            text[index] = b'0';
            index -= 1;
        }
        text[index] += 1;
    }
    if text[0] == b'1' {
        exponent += 1;
    } else {
        text.remove(0);
    }

    // 0x8014F6A0.
    let point = if exponent <= 0 {
        text.insert(0, b'0');
        1
    } else {
        exponent as usize
    };
    if precision > 0 {
        text.insert(point, b'.');
        if exponent < 0 {
            let zeros = precision.min(exponent.unsigned_abs() as usize);
            for _ in 0..zeros {
                text.insert(point + 1, b'0');
            }
        }
    }
    if negative {
        text.insert(0, b'-');
    }

    text
}

/// 0x8014FC70 with 0x8014FB90 and 0x8014FD18.
fn digits(value: f64) -> Digits {
    let (sign_exponent, mantissa) = extend(value);
    let negative = sign_exponent & 0x8000 != 0;
    let exponent = sign_exponent & 0x7FFF;
    let high = (mantissa >> 32) as u32;
    let low = mantissa as u32;
    let zero = || Digits {
        negative,
        exponent: 0,
        digits: alloc::vec![b'0'],
    };

    if exponent == 0 && mantissa == 0 {
        return zero();
    }
    if exponent == 0x7FFF {
        let plain = high == 0x8000_0000 && low == 0;
        let text: &[u8] = if !plain && high & 0x4000_0000 == 0 {
            b"1#SNAN"
        } else if negative && high == 0xC000_0000 {
            if low == 0 { b"1#IND" } else { b"1#QNAN" }
        } else if plain {
            b"1#INF"
        } else {
            b"1#QNAN"
        };
        return Digits {
            negative,
            exponent: 1,
            digits: text.to_vec(),
        };
    }

    let mut x = Ld12((u128::from(exponent) << 80) | (u128::from(mantissa) << 16));
    let estimate = (((high >> 23) & 0x1FE) + (u32::from(exponent) >> 8))
        .wrapping_mul(0x4D)
        .wrapping_add(u32::from(exponent).wrapping_mul(0x4D10))
        .wrapping_sub(0x1343_12F4);
    let mut decimal = i32::from(((estimate as i32) >> 16) as i16);
    x = scale_by_ten(x, -decimal);
    if x.exponent() >= 0x3FFF {
        decimal += 1;
        x = multiply(x, TENTH);
    }

    let shift = i32::from(x.exponent()) - 0x3FFE;
    let mut bits = ((x.0 & MANTISSA) << 8) & LOW_96;
    let right = shift.unsigned_abs() & 0xFF;
    if shift < 0 && right > 0 {
        bits >>= right;
    }

    let mut out = Vec::with_capacity(SIGNIFICANT_DIGITS + 1);
    for _ in 0..=SIGNIFICANT_DIGITS {
        let copy = bits;
        bits = (bits << 2) & LOW_96;
        bits = (bits + copy) & LOW_96;
        bits = (bits << 1) & LOW_96;
        out.push(b'0'.wrapping_add((bits >> 88) as u8));
        bits &= !(0xFF << 88);
    }

    // The last digit rounds the others half up; trailing zeros go.
    let last = out.pop().unwrap_or(b'0');
    if last >= b'5' {
        let mut kept = out.len();
        while kept > 0 && out[kept - 1] == b'9' {
            out[kept - 1] = b'0';
            kept -= 1;
        }
        if kept == 0 {
            kept = 1;
            decimal += 1;
        }
        out[kept - 1] += 1;
        out.truncate(kept);
    } else {
        while out.last() == Some(&b'0') {
            out.pop();
        }
        if out.is_empty() {
            return zero();
        }
    }

    Digits {
        negative,
        exponent: decimal,
        digits: out,
    }
}

/// 0x8014FB90: sign and exponent, and the 64-bit mantissa with its
/// integer bit; denormals are normalized.
fn extend(value: f64) -> (u16, u64) {
    let bits = value.to_bits();
    let sign = ((bits >> 48) as u16) & 0x8000;
    let exponent = ((bits >> 52) & 0x7FF) as u16;
    let fraction = bits & ((1 << 52) - 1);
    if exponent == 0 && fraction == 0 {
        return (sign, 0);
    }

    let (mut biased, integer) = match exponent {
        0 => (0x3C01u16, 0),
        0x7FF => (0x7FFF, 1u64 << 63),
        _ => (exponent + 0x3C00, 1 << 63),
    };
    let mut mantissa = (fraction << 11) | integer;
    while mantissa & (1 << 63) == 0 {
        mantissa <<= 1;
        biased = biased.wrapping_sub(1);
    }

    (sign | biased, mantissa)
}

/// `__multtenpow12` 0x80150530 with the multiply flag set: `x * 10^power`
/// from the powers in groups of octal digits.
fn scale_by_ten(mut x: Ld12, power: i32) -> Ld12 {
    let (table, mut remaining) = match power {
        0 => return x,
        power if power < 0 => (&NEGATIVE, power.unsigned_abs()),
        power => (&POSITIVE, power.unsigned_abs()),
    };

    let mut group = 0;
    while remaining != 0 {
        let index = (remaining & 7) as usize;
        remaining >>= 3;
        if index != 0 {
            // Doubles need no more than three octal digits.
            let Some(&entry) = table.get(group * 7 + index - 1) else {
                return x;
            };
            let mut factor = Ld12(entry);
            // A rounded-up entry is used one unit lower.
            if factor.word(0) >= 0x8000 {
                let middle = ((factor.0 >> 16) as u32).wrapping_sub(1);
                factor.0 = (factor.0 & !(0xFFFF_FFFF << 16)) | (u128::from(middle) << 16);
            }
            x = multiply(x, factor);
        }
        group += 1;
    }

    x
}

/// `__ld12mul` 0x80150248: the product from 16-bit partial products,
/// truncated below the top 96 bits, normalized and rounded.
fn multiply(x: Ld12, y: Ld12) -> Ld12 {
    let (sign_x, sign_y) = (x.exponent(), y.exponent());
    let sign = (sign_x ^ sign_y) & 0x8000;
    let (exponent_x, exponent_y) = (sign_x & 0x7FFF, sign_y & 0x7FFF);
    let mut sum = exponent_x + exponent_y;
    if exponent_x >= 0x7FFF || exponent_y >= 0x7FFF || sum > 0xBFFD {
        return Ld12::infinity(sign);
    }
    if sum <= 0x3FBF {
        return Ld12(0);
    }

    let magnitude = |value: Ld12| value.0 & !(1u128 << 95);
    if exponent_x == 0 {
        sum += 1;
        if magnitude(x) == 0 {
            return Ld12(0);
        }
    }
    if exponent_y == 0 {
        sum += 1;
        if magnitude(y) == 0 {
            return Ld12(0);
        }
    }

    // A carry out of the 32-bit window lands one word higher without
    // carrying further; the top window's carry is lost.
    let mut product = [0u16; 7];
    for i in 0..5 {
        for k in 0..5 - i {
            let partial = u32::from(x.word(i + k)) * u32::from(y.word(4 - k));
            let window = u32::from(product[i]) | (u32::from(product[i + 1]) << 16);
            let (total, carry) = window.overflowing_add(partial);
            product[i] = total as u16;
            product[i + 1] = (total >> 16) as u16;
            if carry {
                product[i + 2] = product[i + 2].wrapping_add(1);
            }
        }
    }
    let mut bits = product[..6]
        .iter()
        .rev()
        .fold(0u128, |bits, &word| (bits << 16) | u128::from(word));

    let mut exponent = sum.wrapping_sub(0x3FFE);
    while (exponent as i16) > 0 && bits & (1 << 95) == 0 {
        bits = (bits << 1) & LOW_96;
        exponent = exponent.wrapping_sub(1);
    }
    if (exponent as i16) <= 0 {
        exponent = exponent.wrapping_sub(1);
        let mut sticky = false;
        while (exponent as i16) < 0 {
            sticky |= bits & 1 != 0;
            bits >>= 1;
            exponent = exponent.wrapping_add(1);
        }
        if sticky {
            bits |= 1;
        }
    }

    if bits & 0xFFFF > 0x8000 || bits & 0x1_FFFF == 0x1_8000 {
        let middle = (bits >> 16) as u32;
        let high = (bits >> 48) as u32;
        let top = (bits >> 80) as u16;
        bits &= 0xFFFF;
        let (middle, high, top) = if middle != u32::MAX {
            (middle + 1, high, top)
        } else if high != u32::MAX {
            (0, high + 1, top)
        } else if top != 0xFFFF {
            (0, 0, top + 1)
        } else {
            exponent = exponent.wrapping_add(1);
            (0, 0, 0x8000)
        };
        bits |= (u128::from(middle) << 16) | (u128::from(high) << 48) | (u128::from(top) << 80);
    }

    if exponent >= 0x7FFF {
        return Ld12::infinity(sign);
    }

    Ld12((u128::from(sign | exponent) << 80) | (bits >> 16))
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use alloc::vec::Vec;

    use super::*;

    fn fixed(value: f64) -> String {
        String::from_utf8(format_fixed(value, 6)).unwrap()
    }

    #[test]
    fn property_values_format_like_the_kernel() {
        let values = [
            0.0,
            -0.0,
            1.0,
            -1.0,
            14.0,
            18.0,
            11.52f32 as f64,
            0.1,
            0.5,
            0.0000005,
            0.0000004999,
            9.9999995,
            123456.789,
            1e20,
            2.5e-7,
            -3.25,
        ];
        let text: Vec<String> = values.iter().map(|&value| fixed(value)).collect();

        insta::assert_snapshot!(text.join("\n"));
    }

    #[test]
    fn digit_strings_have_seventeen_digits_rounded_half_up() {
        let text: Vec<String> = [11.52f32 as f64, 0.1, 1.0 / 3.0, 123456.789, 5e-324]
            .iter()
            .map(|&value| {
                let digits = digits(value);
                let text = String::from_utf8(digits.digits).unwrap();
                alloc::format!("{text} e{}", digits.exponent)
            })
            .collect();

        insta::assert_snapshot!(text.join("\n"));
    }

    #[test]
    fn non_finite_values_format_as_the_kernel_names_them() {
        let text: Vec<String> = [f64::INFINITY, f64::NEG_INFINITY, f64::NAN]
            .iter()
            .map(|&value| fixed(value))
            .collect();

        insta::assert_snapshot!(text.join("\n"));
    }
}
