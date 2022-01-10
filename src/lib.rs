extern crate tokio;

use core::mem;
use std::collections::LinkedList;

use tokio::io::AsyncReadExt;
use tokio::io::Result as IoResult;

const NAL_UNIT_PREFIX_NULL_BYTES: usize = 2;

/// H.264 stream reader from tokio
///
/// Features:
///   - Splits the h.264 stream into its constituent NAL units so implementers can split/stitch NAL units at the stream level
pub struct H264Stream<R> {
	reader: R,
	buffer: Vec<u8>,
	unit_buffer: LinkedList<H264NalUnit>,
	nal_unit_detect: usize,
}

impl<R: AsyncReadExt + Unpin> H264Stream<R> {
	/// Constructs the h264 stream reader from an existing tokio async reader, and allocates 4MiB to an internal buffer
	pub fn new(reader: R) -> Self {
		H264Stream {
			reader,
			// initial 4MiB buffer
			buffer: Vec::with_capacity(4 << 20),
			unit_buffer: LinkedList::new(),
			nal_unit_detect: 0,
		}
	}

	/// Store-and-forward implementation of parsing H264 NAL Units
	///
	/// It always returns a NAL unit that only has 2 leading null bytes
	pub async fn next(&mut self) -> IoResult<H264NalUnit> {
		loop {
			if let Some(unit) = self.unit_buffer.pop_front() {
				return Ok(unit);
			}

			let start = self.buffer.len();
			let read = self.reader.read_buf(&mut self.buffer).await?;
			let end = start + read;
			for i in start..end {
				// H264 NAL Unit Header is 0x000001 https://stackoverflow.com/a/2861340/8835688
				if self.buffer[i] == 0x00 {
					self.nal_unit_detect += 1;
					continue;
				}

				// Some encoder implementations write more than 2 null bytes
				let is_nal_header = self.nal_unit_detect >= NAL_UNIT_PREFIX_NULL_BYTES && self.buffer[i] == 0x01;
				let nal_unit_detect = mem::replace(&mut self.nal_unit_detect, 0);

				if is_nal_header {
					// Side effect of this is that the nal units emitted here always only has 2 leading null bytes
					let last_frame_end = i - nal_unit_detect;
					// If we're at the start of the h264 stream there's no previous unit to emit
					if last_frame_end == 0 {
						continue;
					}

					// Extract NAL unit
					let last_frame_start = nal_unit_detect - NAL_UNIT_PREFIX_NULL_BYTES;
					let mut nal_unit = Vec::with_capacity(last_frame_end);
					nal_unit.extend(&self.buffer[last_frame_start..last_frame_end]);


					// Move to the start (with allocation)
					{
						let mut buffered = Vec::with_capacity(end - last_frame_end);
						buffered.extend(&self.buffer[last_frame_end..end]);
						self.buffer.clear();
						self.buffer.extend(&buffered);
					}

					self.unit_buffer.push_back(H264NalUnit::new(nal_unit));
				}
			}
		}
	}
}

#[derive(Clone, Debug)]
pub struct H264NalUnit {
	/// The NAL unit code for this current unit (values are 0-31 inclusive)
	///
	/// `7` and `8` should be cached across the entire video session
	/// `5` should be cached until we see the next `5` NAL unit
	pub unit_code: u8,
	/// The underlying bytes of this NAL unit, unparsed.
	pub raw_bytes: Vec<u8>,
}

impl H264NalUnit {
	/// Constructs a new NAL unit from the raw bytes (interpret the unicode and store the bytes)
	pub fn new(raw_bytes: Vec<u8>) -> Self {
		H264NalUnit {
			// There's 32 possible NAL unit codes for H264
			unit_code: 0x1f & raw_bytes[3],
			raw_bytes,
		}
	}
}
