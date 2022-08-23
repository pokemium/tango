use crate::{protocol, signaling};

#[derive(Debug)]
pub enum Error {
    ExpectedHello,
    ProtocolVersionTooOld,
    ProtocolVersionTooNew,
    Other(anyhow::Error),
}

impl From<anyhow::Error> for Error {
    fn from(err: anyhow::Error) -> Self {
        Error::Other(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::Other(err.into())
    }
}

impl From<datachannel_wrapper::Error> for Error {
    fn from(err: datachannel_wrapper::Error) -> Self {
        Error::Other(err.into())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Error::ExpectedHello => write!(f, "expected hello"),
            Error::ProtocolVersionTooOld => write!(f, "protocol version too old"),
            Error::ProtocolVersionTooNew => write!(f, "protocol version too new"),
            Error::Other(e) => write!(f, "other error: {}", e),
        }
    }
}

impl std::error::Error for Error {}

pub async fn negotiate(
    session_id: &str,
    signaling_connect_addr: &str,
    ice_servers: &[String],
) -> Result<
    (
        datachannel_wrapper::DataChannel,
        datachannel_wrapper::PeerConnection,
    ),
    Error,
> {
    log::info!(
        "negotiating match, session_id = {}, ice_servers = {:?}",
        session_id,
        ice_servers
    );

    let (mut peer_conn, mut event_rx) =
        datachannel_wrapper::PeerConnection::new(datachannel_wrapper::RtcConfig::new(ice_servers))?;

    let dc = peer_conn.create_data_channel(
        "tango",
        datachannel_wrapper::DataChannelInit::default()
            .reliability(datachannel_wrapper::Reliability {
                unordered: false,
                unreliable: false,
                max_packet_life_time: 0,
                max_retransmits: 0,
            })
            .negotiated()
            .manual_stream()
            .stream(0),
    )?;

    loop {
        if let Some(datachannel_wrapper::PeerConnectionEvent::GatheringStateChange(
            datachannel_wrapper::GatheringState::Complete,
        )) = event_rx.recv().await
        {
            break;
        }
    }

    log::info!("candidates gathered");

    signaling::connect(signaling_connect_addr, &mut peer_conn, event_rx, session_id).await?;

    let (mut dc_tx, mut dc_rx) = dc.split();

    log::debug!(
        "local sdp (type = {:?}): {}",
        peer_conn.local_description().expect("local sdp").sdp_type,
        peer_conn.local_description().expect("local sdp").sdp
    );
    log::debug!(
        "remote sdp (type = {:?}): {}",
        peer_conn.remote_description().expect("remote sdp").sdp_type,
        peer_conn.remote_description().expect("remote sdp").sdp
    );

    dc_tx
        .send(
            protocol::Packet::Hello(protocol::Hello {
                protocol_version: protocol::VERSION,
            })
            .serialize()
            .expect("serialize")
            .as_slice(),
        )
        .await?;

    let hello = match protocol::Packet::deserialize(
        match dc_rx.receive().await {
            Some(d) => d,
            None => {
                return Err(Error::ExpectedHello);
            }
        }
        .as_slice(),
    )
    .map_err(|_| Error::ExpectedHello)?
    {
        protocol::Packet::Hello(hello) => hello,
        _ => {
            return Err(Error::ExpectedHello);
        }
    };

    if hello.protocol_version < protocol::VERSION {
        return Err(Error::ProtocolVersionTooOld);
    }

    if hello.protocol_version > protocol::VERSION {
        return Err(Error::ProtocolVersionTooNew);
    }

    Ok((dc_rx.unsplit(dc_tx), peer_conn))
}

pub struct Transport {
    dc_tx: datachannel_wrapper::DataChannelSender,
    rendezvous_rx: Option<tokio::sync::oneshot::Receiver<()>>,
}

impl Transport {
    pub fn new(
        dc_tx: datachannel_wrapper::DataChannelSender,
        rendezvous_rx: tokio::sync::oneshot::Receiver<()>,
    ) -> Transport {
        Transport {
            dc_tx,
            rendezvous_rx: Some(rendezvous_rx),
        }
    }

    pub async fn send_input(
        &mut self,
        round_number: u8,
        local_tick: u32,
        tick_diff: i8,
        joyflags: u16,
    ) -> anyhow::Result<()> {
        self.dc_tx
            .send(
                protocol::Packet::Input(protocol::Input {
                    round_number,
                    local_tick,
                    tick_diff,
                    joyflags,
                })
                .serialize()?
                .as_slice(),
            )
            .await?;
        if let Some(rendezvous_rx) = self.rendezvous_rx.take() {
            rendezvous_rx.await?;
        }
        Ok(())
    }
}
