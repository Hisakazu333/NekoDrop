#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChunkRange {
    pub offset: u64,
    pub len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChunkPlan {
    pub file_size: u64,
    pub chunk_size: u64,
}

impl ChunkPlan {
    pub fn new(file_size: u64, chunk_size: u64) -> Self {
        Self {
            file_size,
            chunk_size: chunk_size.max(1),
        }
    }

    pub fn ranges(&self) -> Vec<ChunkRange> {
        let mut ranges = Vec::new();
        let mut offset = 0;

        while offset < self.file_size {
            let remaining = self.file_size - offset;
            let len = remaining.min(self.chunk_size);
            ranges.push(ChunkRange { offset, len });
            offset += len;
        }

        ranges
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_final_short_chunk() {
        let ranges = ChunkPlan::new(10, 4).ranges();
        assert_eq!(
            ranges,
            vec![
                ChunkRange { offset: 0, len: 4 },
                ChunkRange { offset: 4, len: 4 },
                ChunkRange { offset: 8, len: 2 },
            ]
        );
    }
}
