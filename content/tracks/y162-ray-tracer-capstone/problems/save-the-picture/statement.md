**Capstone, step 4 of 9.** The starter contains reference code for steps 1 to 3. This step turns numbers into a file you can open. No image library.

## From light to bytes

The renderer works in **linear** light: twice the number is twice the light. Screens expect sRGB. For one channel `c`:

1. NaN counts as 0. Then limit `c` to `0..=1`.
2. `e = 12.92 c` if `c <= 0.0031308`, else `e = 1.055 c^(1/2.4) - 0.055`.
3. The byte is `255 e`, rounded to the nearest whole number.

`to_rgb8` does this for every channel, pixel by pixel, red then green then blue.

## The PNG file

`rgb` holds the lines from the top, each from the left, three bytes per pixel. All PNG numbers are **big endian**.

- Start with the 8 bytes `137 80 78 71 13 10 26 10`.
- Then **chunks**: length of the data (u32), a 4-letter type, the data, and a CRC-32 of **type and data**.
- `IHDR` comes first: width (u32), height (u32), then the bytes `8 2 0 0 0`.
- Then one or more `IDAT` chunks. Joined, their data is one zlib stream.
- Last, an empty `IEND`. Write no other chunks.

**The unpacked data** is, for each line, one byte `0` followed by the line's colour bytes.

**The zlib stream** is two header bytes (`0x78 0x01`), then deflate blocks, then the Adler-32 of the unpacked data (u32, big endian). Use **stored** blocks: one byte (`1` on the last block, `0` before it), the length `n` (u16, little endian), `!n` (u16, little endian), then `n` bytes. So a block holds at most 65535 bytes.

## The checksums

- **CRC-32:** start with all 32 bits set. For each byte, XOR it into the low byte. Then 8 times: shift right by one, and if the bit that fell out was 1, XOR with `0xEDB88320`. Invert at the end.
- **Adler-32:** `a = 1`, `b = 0`. For each byte, `a = (a + byte) % 65521`, then `b = (b + a) % 65521`. The result is `b * 65536 + a`.

## Refusals

`encode_png` returns `None` if a side is 0 or above `2^31 - 1`, or if `rgb.len()` is not exactly `width * height * 3`. No size may panic.

The tests read your file with their own strict reader. At an exact halfway point either byte is accepted.
