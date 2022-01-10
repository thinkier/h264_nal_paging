use crate::H264Stream;

#[tokio::test]
async fn scan_over_file() {
	let stream = tokio::fs::OpenOptions::new()
		.read(true)
		.open("./test.h264")
		.await
		.unwrap();

	let mut h264 = H264Stream::new(stream);

	while let Ok(nal) = h264.next().await {
		print!("{}", nal.unit_code);

		if nal.unit_code == 7 || nal.unit_code == 8 {
			println!(": {:?}", nal);
		}else{
			println!();
		}
	}
}
