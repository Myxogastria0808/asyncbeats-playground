#[derive(Debug, Clone)]
pub enum ChannelEnum {
    Monaural,
    Stereo,
}

#[derive(Debug, Clone)]
pub struct AudioInfo {
    pub channel: ChannelEnum,
    pub chunk_size: u64,
}
