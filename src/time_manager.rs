use alloc::rc::Rc;
use core::cell::RefCell;
use embassy_time::{Instant, Duration as EmbassyDuration};
use chrono::{DateTime, Utc, Duration as ChronoDuration};
use embassy_net::{IpEndpoint, Ipv4Address, Stack};
use embassy_net::udp::{UdpSocket, PacketMetadata};

pub struct TimeManager {
    // Stores the pair of (local_monotonic_time, real_world_time) at the moment of sync
    state: RefCell<Option<(Instant, DateTime<Utc>)>>,
}

pub type SharedTimeManager = Rc<TimeManager>;

impl TimeManager {
    pub fn new(initial_time: Option<DateTime<Utc>>) -> Self {
        let state = if let Some(dt) = initial_time {
            Some((Instant::now(), dt))
        } else {
            None
        };
        
        Self {
            state: RefCell::new(state),
        }
    }


    pub fn get_time(&self) -> Option<DateTime<Utc>> {
        let state = self.state.borrow();
        let (sync_instant, sync_time) = *state.as_ref()?;
        
        let now = Instant::now();
        // Handle potential monotonicity issues if any (unlikely with Instant)
        let elapsed = now.duration_since(sync_instant);
        
        // Convert to chrono duration. elapsed.as_micros() returns u64.
        // i64 max micros is ~300,000 years, so safe.
        let elapsed_micros = elapsed.as_micros() as i64;
        let chrono_delta = ChronoDuration::microseconds(elapsed_micros);
        
        Some(sync_time + chrono_delta)
    }

    pub fn set_time(&self, dt: DateTime<Utc>) {
        *self.state.borrow_mut() = Some((Instant::now(), dt));
    }

    // Called periodically by time_sync_task
    pub async fn sync_time(&self, stack: Stack<'_>, port: u16) -> Result<(), ()> {
        defmt::info!("Starting NTP sync...");
        
        let mut rx_meta = [PacketMetadata::EMPTY; 1];
        let mut rx_buffer = [0u8; 64];
        let mut tx_meta = [PacketMetadata::EMPTY; 1];
        let mut tx_buffer = [0u8; 64];
        
        // Stack typically implements Clone (shallow copy of handle) or reference is enough.
        // If stack is Stack<'static>, it might be Copy/Clone. 
        // User used stack.clone(), implying it is possible.
        let mut socket = UdpSocket::new(stack, &mut rx_meta, &mut rx_buffer, &mut tx_meta, &mut tx_buffer);
        if let Err(e) = socket.bind(port) {
            defmt::error!("Bind failed: {:?}", e);
            return Err(());
        }

        // pool.ntp.org (Using a known IP for now as DNS resolution is separate step)
        // time.google.com: 216.239.35.0
        let remote_endpoint = IpEndpoint::new(embassy_net::IpAddress::Ipv4(Ipv4Address::new(216, 239, 35, 0)), 123);

        let mut buf = [0u8; 48];
        buf[0] = 0x1B; // VN=3, Mode=3 (Client)

        if let Err(e) = socket.send_to(&buf, remote_endpoint).await {
            defmt::error!("Send failed: {:?}", e);
            return Err(());
        }

        let mut buf = [0u8; 48];
        match embassy_time::with_timeout(EmbassyDuration::from_secs(30), socket.recv_from(&mut buf)).await {
             Ok(Ok((size, _))) => {
                 if size >= 48 {
                     let seconds = u32::from_be_bytes(buf[40..44].try_into().unwrap());
                     let fraction = u32::from_be_bytes(buf[44..48].try_into().unwrap());
                     
                     // 1900 to 1970
                     let delta_seconds = 2_208_988_800u32;
                     
                     if seconds > delta_seconds {
                        let seconds_unix = (seconds - delta_seconds) as i64;
                        // fraction to nanos: fraction / 2^32 * 1e9
                        let nanos = ((fraction as u64 * 1_000_000_000) >> 32) as u32;

                        if let Some(dt) = DateTime::from_timestamp(seconds_unix, nanos) {
                             let now = Instant::now();
                             defmt::info!("NTP Sync Success: {}", seconds_unix);
                             *self.state.borrow_mut() = Some((now, dt));
                             return Ok(());
                        }
                     }
                 }
             }
             Ok(Err(e)) => {
                 defmt::error!("Recv failed: {:?}", e);
             }
             Err(_) => {
                 defmt::error!("NTP Timeout");
             }
        }
        
        Err(())
    }
}
