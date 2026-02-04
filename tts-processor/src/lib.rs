use color_eyre::eyre::Result;
use rkyv::ser::Serializer;
use rkyv::{Archive, Deserialize, Serialize};

/// Commands that can be sent to the TTS processor
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(compare(PartialEq), check_bytes)]
pub enum TtsCommand {
    /// Generate audio from text and play it
    GenerateAudio(String),
    /// Stop current playback
    Stop,
    /// Wait until current audio playback is finished
    WaitUntilFinished,
}

/// Responses from the TTS processor
#[derive(Debug, Clone, Archive, Serialize, Deserialize)]
#[archive(compare(PartialEq), check_bytes)]
pub enum TtsResponse {
    /// Generation started
    Started,
    /// Chunk generated (optional progress indicator)
    ChunkGenerated(u32),
    /// Generation and playback complete
    Finished,
    /// Playback stopped
    Stopped,
    /// Error occurred
    Error(String),
}

/// Serialize a command to bytes
pub fn serialize_command(cmd: &TtsCommand) -> Result<Vec<u8>> {
    use rkyv::ser::serializers::AllocSerializer;
    let mut serializer = AllocSerializer::<256>::default();
    serializer
        .serialize_value(cmd)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to serialize command: {}", e))?;
    let aligned_vec = serializer.into_serializer().into_inner();
    Ok(aligned_vec.as_slice().to_vec())
}

/// Deserialize a command from bytes
pub fn deserialize_command(bytes: &[u8]) -> Result<TtsCommand> {
    let archived = rkyv::check_archived_root::<TtsCommand>(bytes)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to check archived command: {}", e))?;
    archived
        .deserialize(&mut rkyv::Infallible)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to deserialize command: {}", e))
}

/// Serialize a response to bytes
pub fn serialize_response(resp: &TtsResponse) -> Result<Vec<u8>> {
    use rkyv::ser::serializers::AllocSerializer;
    let mut serializer = AllocSerializer::<256>::default();
    serializer
        .serialize_value(resp)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to serialize response: {}", e))?;
    let aligned_vec = serializer.into_serializer().into_inner();
    Ok(aligned_vec.as_slice().to_vec())
}

/// Deserialize a response from bytes
pub fn deserialize_response(bytes: &[u8]) -> Result<TtsResponse> {
    let archived = rkyv::check_archived_root::<TtsResponse>(bytes)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to check archived response: {}", e))?;
    archived
        .deserialize(&mut rkyv::Infallible)
        .map_err(|e| color_eyre::eyre::eyre!("Failed to deserialize response: {}", e))
}
