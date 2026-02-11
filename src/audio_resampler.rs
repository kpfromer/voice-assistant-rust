pub struct AudioResampler {
    input_rate: u32,
    output_rate: u32,
    // TODO: handle multiple channels properly
    #[allow(dead_code)]
    channels: u16,
    buffer: Vec<f32>,
    target_chunk_size: usize,
}

impl AudioResampler {
    pub fn new(input_rate: u32, output_rate: u32, channels: u16, target_chunk_size: usize) -> Self {
        Self {
            input_rate,
            output_rate,
            channels,
            buffer: Vec::new(),
            target_chunk_size,
        }
    }

    // Simple linear interpolation resampling
    pub fn resample(&mut self, input: &[f32]) -> Vec<Vec<f32>> {
        let ratio = self.output_rate as f64 / self.input_rate as f64;
        let output_len = (input.len() as f64 * ratio) as usize;

        let mut resampled = Vec::with_capacity(output_len);

        for i in 0..output_len {
            let src_index = i as f64 / ratio;
            let src_index_floor = src_index.floor() as usize;
            let src_index_ceil = (src_index_floor + 1).min(input.len() - 1);
            let frac = src_index - src_index_floor as f64;

            let sample =
                input[src_index_floor] * (1.0 - frac) as f32 + input[src_index_ceil] * frac as f32;
            resampled.push(sample);
        }

        // Add to buffer
        self.buffer.extend_from_slice(&resampled);

        // Extract chunks of target size
        let mut chunks = Vec::new();
        while self.buffer.len() >= self.target_chunk_size {
            let chunk: Vec<f32> = self.buffer.drain(..self.target_chunk_size).collect();
            chunks.push(chunk);
        }

        chunks
    }
}
