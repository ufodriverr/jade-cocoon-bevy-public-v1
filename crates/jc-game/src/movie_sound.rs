
use std::collections::VecDeque;
use std::num::NonZero;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::audio::{ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;

/// Seconds of silence a line starts with. A sector's sound lasts exactly until the next
/// audio sector is due, so without a lead every pass of the app that comes a little late
/// would leave a gap.
pub const LEAD: f64 = 0.1;
/// Seconds of sound a line holds at most. More than that waits only when nothing takes
/// the samples (no sound device); then the oldest are dropped.
const LONGEST_WAIT: f64 = 2.0;

/// How loud the sound of a line is heard, and whether it is held, as the sound thread
/// takes it: the build's own volumes and its pause for a PC (`audio.rs`). The sound
/// thread reads it at every sample, so a change is heard at once, also of the sound
/// that already waits on a line. Several lines can share one.
#[derive(Debug)]
pub struct Gain {
    /// The share of full loudness, the bits of an `f32`.
    share: AtomicU32,
    /// Nothing is taken from the line, and silence is heard: the sound waits.
    held: AtomicBool,
}

impl Default for Gain {
    fn default() -> Self {
        Self {
            share: AtomicU32::new(1.0f32.to_bits()),
            held: AtomicBool::new(false),
        }
    }
}

impl Gain {
    pub fn share(&self) -> f32 {
        f32::from_bits(self.share.load(Ordering::Relaxed))
    }

    pub fn set_share(&self, share: f32) {
        self.share
            .store(share.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn held(&self) -> bool {
        self.held.load(Ordering::Relaxed)
    }

    pub fn set_held(&self, held: bool) {
        self.held.store(held, Ordering::Relaxed);
    }
}

struct Shared {
    /// Left and right sample of every moment that waits to be played.
    waiting: Mutex<VecDeque<[Sample; 2]>>,
    over: AtomicBool,
    /// Moments the sound thread has taken, the lead not counted.
    taken: AtomicU64,
}

/// What a playing movie puts its sound on.
#[derive(Clone)]
pub struct Line {
    shared: Arc<Shared>,
    rate: u32,
    gain: Arc<Gain>,
}

impl Line {
    /// A line for sound of `rate` samples a second, with the lead on it, heard at its
    /// whole loudness.
    #[cfg(test)]
    pub fn new(rate: u32) -> Self {
        Self::with_gain(rate, &Arc::default())
    }

    /// A line for sound of `rate` samples a second, with the lead on it, heard as `gain`
    /// says.
    pub fn with_gain(rate: u32, gain: &Arc<Gain>) -> Self {
        let lead = (f64::from(rate) * LEAD) as usize;
        Self {
            shared: Arc::new(Shared {
                waiting: Mutex::new(std::iter::repeat_n([0.0; 2], lead).collect()),
                over: AtomicBool::new(false),
                taken: AtomicU64::new(0),
            }),
            rate,
            gain: gain.clone(),
        }
    }

    /// Puts the samples of one audio sector on the line: left and right in turn for a
    /// stereo sector, and a mono sector on both sides. `volume` is the share of full
    /// loudness for the left and the right side.
    pub fn push(&self, samples: &[i16], stereo: bool, volume: (f32, f32)) {
        let side = |sample: i16, volume: f32| f32::from(sample) / 32768.0 * volume;
        let Ok(mut waiting) = self.shared.waiting.lock() else {
            return;
        };
        if stereo {
            let pairs = samples.as_chunks::<2>().0;
            waiting.extend(
                pairs
                    .iter()
                    .map(|&[left, right]| [side(left, volume.0), side(right, volume.1)]),
            );
        } else {
            waiting.extend(
                samples
                    .iter()
                    .map(|&sample| [side(sample, volume.0), side(sample, volume.1)]),
            );
        }
        let most = (f64::from(self.rate) * LONGEST_WAIT) as usize;
        let over = waiting.len().saturating_sub(most);
        waiting.drain(..over);
    }

    /// Ends the sound at once, whatever still waits: the player mutes the drive's sound
    /// when its loop is over.
    pub fn close(&self) {
        self.shared.over.store(true, Ordering::Relaxed);
    }

    /// Samples a second of each side.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Moments of sound that wait to be taken, the lead among them.
    pub fn waiting(&self) -> usize {
        self.shared
            .waiting
            .lock()
            .map_or(0, |waiting| waiting.len())
    }

    /// Moments of sound the sound thread has taken so far.
    pub fn taken(&self) -> u64 {
        self.shared.taken.load(Ordering::Relaxed)
    }

    fn flow(&self) -> Flow {
        Flow {
            line: self.clone(),
            lead: (f64::from(self.rate) * LEAD) as u64,
            right: None,
        }
    }
}

/// The sound of a movie as bevy plays it.
#[derive(Asset, TypePath)]
pub struct MovieSound(pub Line);

impl Decodable for MovieSound {
    type Decoder = Flow;

    fn decoder(&self) -> Flow {
        self.0.flow()
    }
}

/// The end of a line the sound thread reads: left and right sample in turn at the line's
/// gain, silence while nothing waits or the gain holds the line, and the end once the
/// line is closed.
pub struct Flow {
    line: Line,
    /// Moments of the lead still to come.
    lead: u64,
    /// The right sample of the moment whose left one was just given.
    right: Option<Sample>,
}

impl Iterator for Flow {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(right) = self.right.take() {
            return Some(right);
        }
        let shared = &self.line.shared;
        if shared.over.load(Ordering::Relaxed) {
            return None;
        }
        let gain = &self.line.gain;
        if gain.held() {
            self.right = Some(0.0);
            return Some(0.0);
        }
        let [left, right] = match shared.waiting.lock().ok()?.pop_front() {
            Some(moment) => {
                if self.lead > 0 {
                    self.lead -= 1;
                } else {
                    shared.taken.fetch_add(1, Ordering::Relaxed);
                }
                moment
            }
            None => [0.0; 2],
        };
        let share = gain.share();
        self.right = Some(right * share);
        Some(left * share)
    }
}

impl Source for Flow {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::<u16>::MIN.saturating_add(1)
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.line.rate).unwrap_or(NonZero::<u32>::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_line_gives_its_lead_then_the_sound_and_silence_while_nothing_waits() {
        let line = Line::new(18_900);
        let mut flow = line.flow();
        assert_eq!(
            (flow.channels().get(), flow.sample_rate().get()),
            (2, 18_900)
        );
        // A mono sector sounds on both sides, each at its own volume.
        line.push(&[16384, -32768], false, (1.0, 0.5));
        let lead: Vec<Sample> = flow.by_ref().take(2 * 1890).collect();
        assert!(lead.iter().all(|&sample| sample == 0.0));
        assert_eq!(line.taken(), 0);
        let sound: Vec<Sample> = flow.by_ref().take(4).collect();
        assert_eq!(sound, [0.5, 0.25, -1.0, -0.5]);
        assert_eq!(line.taken(), 2);
        // Nothing waits: silence, and nothing is counted as taken.
        assert_eq!(flow.by_ref().take(3).collect::<Vec<_>>(), [0.0; 3]);
        assert_eq!(line.taken(), 2);

        // A stereo sector has left and right in turn. The pair that comes while the flow
        // stands between a left and a right sample does not swap the sides.
        line.push(&[16384, -16384, 8192, 0], true, (1.0, 1.0));
        assert_eq!(
            flow.by_ref().take(5).collect::<Vec<_>>(),
            [0.0, 0.5, -0.5, 0.25, 0.0]
        );

        // A closed line ends, whatever waits.
        line.push(&[1, 1], true, (1.0, 1.0));
        line.close();
        assert_eq!(flow.next(), None);
    }

    #[test]
    fn a_line_nobody_takes_from_drops_its_oldest_sound() {
        let line = Line::new(1000);
        for value in 0..5i16 {
            line.push(&[value * 1000; 1000], false, (1.0, 1.0));
        }
        // Two seconds wait at most: the lead and the first sectors are gone.
        assert_eq!(line.shared.waiting.lock().unwrap().len(), 2000);
        let mut flow = line.flow();
        assert_eq!(flow.next(), Some(3000.0 / 32768.0));
    }

    #[test]
    fn a_gain_turns_a_line_down_at_once_and_holds_it_without_taking() {
        let gain = Arc::new(Gain::default());
        let line = Line::with_gain(10, &gain);
        let mut flow = line.flow();
        line.push(&[16384, -16384, 8192, 8192], true, (1.0, 1.0));
        // The lead of a tenth of a second: one moment.
        assert_eq!(flow.by_ref().take(2).collect::<Vec<_>>(), [0.0; 2]);
        // Half as loud from the next sample on, also for what waits already.
        gain.set_share(0.5);
        assert_eq!(flow.by_ref().take(2).collect::<Vec<_>>(), [0.25, -0.25]);
        // Held: silence, and nothing is taken from the line.
        gain.set_held(true);
        assert_eq!(flow.by_ref().take(4).collect::<Vec<_>>(), [0.0; 4]);
        assert_eq!((line.waiting(), line.taken()), (1, 1));
        // Let go, the line goes on where it was; a share is never more than the whole.
        gain.set_held(false);
        gain.set_share(3.0);
        assert_eq!(gain.share(), 1.0);
        assert_eq!(flow.by_ref().take(2).collect::<Vec<_>>(), [0.25, 0.25]);
    }
}
