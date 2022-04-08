use std::sync::mpsc::channel;
use std::time::Duration;

use tokio::time::interval;

use crate::H264Stream;

#[tokio::test]
async fn no_encapsulated_units() {
	let (tx, rx) = channel();

	let hand = tokio::spawn(async move {
		let stream = tokio::fs::OpenOptions::new()
			.read(true)
			.open("./test.h264")
			.await
			.unwrap();

		let mut h264 = H264Stream::new(stream);

		while let Ok(unit) = h264.try_next().await {
			if let Some(nal) = unit {
				let mut nulls = 0;
				nal.raw_bytes.iter().skip(3).for_each(|byte| {
					if *byte == 0x00 {
						nulls += 1;
					} else if nulls >= 2 && *byte == 0x01 {
						let _ = tx.send(true);
					} else {
						nulls = 0;
					}
				});
				let _ = tx.send(false);
			}
		}
	});

	let mut int = interval(Duration::from_millis(500));

	loop {
		int.tick().await;
		let mut received = false;
		while let Ok(x) = rx.try_recv() {
			if x {
				hand.abort();
				panic!("Detected unit within unit; Parser isn't working as intended.");
			}
			received = true;
		}

		if !received {
			// Timeout exit
			hand.abort();
			return;
		}
	}
}
