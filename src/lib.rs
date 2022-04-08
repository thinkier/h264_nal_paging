extern crate tokio;

use core::mem;
use std::collections::VecDeque;

use tokio::io::AsyncReadExt;
use tokio::io::Result as IoResult;

const NAL_UNIT_PREFIX_NULL_BYTES: usize = 2;
const READ_BEHIND: usize = NAL_UNIT_PREFIX_NULL_BYTES;

/// H.264 stream reader from tokio
///
/// Features:
///   - Splits the h.264 stream into its constituent NAL units so implementers can split/stitch NAL units at the stream level
pub struct H264Stream<R> {
	reader: R,
	byte_buf: Vec<u8>,
	nulls: usize,
	unit_buf: VecDeque<H264NalUnit>,
}

impl<R: AsyncReadExt + Unpin> H264Stream<R> {
	/// Reads upstream into an internal buffer
	async fn read_buf(&mut self) -> IoResult<usize> {
		self.reader.read_buf(&mut self.byte_buf).await
	}

	/// Constructs the h264 stream reader from an existing tokio async reader, and allocates 4MiB to an internal buffer
	pub fn new(reader: R) -> Self {
		H264Stream {
			reader,
			byte_buf: Vec::with_capacity(4 << 20),
			nulls: 0,
			unit_buf: VecDeque::new(),
		}
	}

	/// Store-and-forward implementation of parsing H264 NAL Units
	///
	/// It always returns a NAL unit that only has 2 leading null bytes
	pub async fn next(&mut self) -> IoResult<H264NalUnit> {
		loop {
			if let Some(x) = self.try_next().await? {
				return Ok(x);
			}
		}
	}

	/// Attempts to fetch the next NAL unit in the stream without blocking
	///
	/// It always returns a NAL unit that only has 2 leading null bytes
	pub async fn try_next(&mut self) -> IoResult<Option<H264NalUnit>> {
		if let Some(x) = self.unit_buf.pop_front() {
			return Ok(Some(x));
		}

		let start = self.byte_buf.len();
		let count = self.read_buf().await?;

		// Skip reading the headers at the start of iteration
		let mut offset = 0;
		for i in 0..count {
			let i = start + i - offset;
			if self.byte_buf[i] == 0x00 {
				self.nulls += 1;
				continue;
			}

			let nulls = mem::replace(&mut self.nulls, 0);
			if nulls >= NAL_UNIT_PREFIX_NULL_BYTES && self.byte_buf[i] == 0x01 {
				let start = nulls - NAL_UNIT_PREFIX_NULL_BYTES;
				let end = i - nulls;

				if end > 0 {
					let (unit, retain) = self.byte_buf.split_at(end);
					let retain = Vec::from(retain);
					self.unit_buf.push_back(H264NalUnit::new(Vec::from(&unit[start..])));
					self.byte_buf.clear();
					self.byte_buf.extend(retain);
					offset += end;
				}
			}
		}

		return Ok(None);
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

#[cfg(test)]
mod tests;