//! Backend « loopback » : un pilote de bouclage multicanal existant est
//! découpé en cartes. Ex. avec BlackHole 16ch : carte 1 stéréo = canaux 1-2,
//! carte 2 mono = canal 3, etc. (dans l'ordre des [[card]] du fichier).
//!
//! Les applications doivent envoyer leur son sur les bons canaux du pilote
//! (routage dans l'application, ou « Configuration audio et MIDI » sur macOS).

use antenne_core::CardConfig;

use super::{VirtualBackend, VirtualCard};

#[derive(Debug)]
pub struct LoopbackBackend {
    device: String,
    next_channel: u16,
}

impl LoopbackBackend {
    pub fn new(device: String) -> Self {
        Self {
            device,
            next_channel: 0,
        }
    }
}

impl VirtualBackend for LoopbackBackend {
    fn name(&self) -> &'static str {
        "loopback"
    }

    fn create(&mut self, card: &CardConfig, _sample_rate: u32) -> anyhow::Result<VirtualCard> {
        let first_channel = self.next_channel;
        self.next_channel += card.channels.count();
        tracing::info!(
            card = %card.id,
            device = %self.device,
            "canaux {}..{} du pilote de bouclage",
            first_channel + 1,
            self.next_channel
        );
        Ok(VirtualCard {
            card_id: card.id.clone(),
            capture_patterns: vec![self.device.clone()],
            first_channel,
            handle: None,
        })
    }

    fn destroy(&mut self, _card: &VirtualCard) -> anyhow::Result<()> {
        Ok(())
    }

    fn positional(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antenne_core::Config;

    #[test]
    fn allocates_channels_in_order() {
        let config = Config::from_toml_str(
            r#"
            [[card]]
            id = "a"
            name = "A"
            channels = "stereo"
            [[card]]
            id = "b"
            name = "B"
            channels = "mono"
            [[card]]
            id = "c"
            name = "C"
            "#,
        )
        .unwrap();
        let mut backend = LoopbackBackend::new("BlackHole 16ch".into());
        let firsts: Vec<u16> = config
            .cards
            .iter()
            .map(|c| backend.create(c, 48_000).unwrap().first_channel)
            .collect();
        assert_eq!(firsts, vec![0, 2, 3]);
    }
}
