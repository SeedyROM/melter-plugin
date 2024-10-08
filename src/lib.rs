use nih_plug::prelude::*;
use std::sync::Arc;

mod equalization;
mod filters;
mod nonlinearity;
mod oversampling;

// Constants for oversampling
const MAX_BLOCK_SIZE: usize = 32;
const MAX_OVERSAMPLING_FACTOR: usize = 4;
const DEFAULT_OVERSAMPLING_FACTOR: usize = 1;
const MAX_OVERSAMPLING_TIMES: usize = oversampling_factor_to_times(MAX_OVERSAMPLING_FACTOR);
const MAX_OVERSAMPLED_BLOCK_SIZE: usize = MAX_BLOCK_SIZE * MAX_OVERSAMPLING_TIMES;

/// A macro to load a param into the scratch buffer
macro_rules! param_next_block {
    ($self:expr, $param_name:ident, $block_size:expr) => {{
        let buffer = &mut $self.scratch_buffers.$param_name;
        $self
            .params
            .$param_name
            .smoothed
            .next_block(buffer, $block_size);
        buffer
    }};
}

#[allow(dead_code)]
struct ScratchBuffers {
    gain: [f32; MAX_OVERSAMPLED_BLOCK_SIZE],
    drive: [f32; MAX_OVERSAMPLED_BLOCK_SIZE],
}

impl Default for ScratchBuffers {
    fn default() -> Self {
        Self {
            gain: [0.0; MAX_OVERSAMPLED_BLOCK_SIZE],
            drive: [0.0; MAX_OVERSAMPLED_BLOCK_SIZE],
        }
    }
}

struct Melter {
    params: Arc<MelterParams>,
    oversamplers: Vec<oversampling::Lanczos3Oversampler>,
    dc_blockers: Vec<filters::DCBlocker>,
    parametric_eqs: Vec<equalization::ParametricEQ>,
    scratch_buffers: Box<ScratchBuffers>,
    sample_rate: f32,
}

impl Default for Melter {
    fn default() -> Self {
        Self {
            params: Arc::new(MelterParams::default()),
            oversamplers: Vec::new(),
            dc_blockers: Vec::new(),
            parametric_eqs: Vec::new(),
            scratch_buffers: Box::default(),
            sample_rate: 44100.0,
        }
    }
}

#[derive(Params)]
struct MelterParams {
    // Pre-post equalization
    #[id = "pre_post_eq"]
    pub pre_post_eq: BoolParam,

    // Distortion parameters
    #[id = "gain"]
    pub gain: FloatParam,
    #[id = "drive"]
    pub drive: FloatParam,

    // 3-band parametric EQ
    #[id = "low_boost"]
    pub low_boost: FloatParam,
    #[id = "mid_boost"]
    pub mid_boost: FloatParam,
    #[id = "high_boost"]
    pub high_boost: FloatParam,

    // Oversampling factor
    #[id = "oversampling_factor"]
    pub oversampling_factor: IntParam,
}
impl Default for MelterParams {
    fn default() -> Self {
        Self {
            pre_post_eq: BoolParam::new("Pre-Post EQ", false),

            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(0.0),
                    max: util::db_to_gain(30.0),
                    factor: FloatRange::gain_skew_factor(0.0, 30.0),
                },
            )
            .with_smoother(SmoothingStyle::Logarithmic(50.0))
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),

            drive: FloatParam::new("Drive", 1.0, FloatRange::Linear { min: 0.0, max: 2.0 })
                .with_smoother(SmoothingStyle::Logarithmic(50.0)),

            low_boost: FloatParam::new(
                "Low Boost",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_unit(" dB"),

            mid_boost: FloatParam::new(
                "Mid Boost",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_unit(" dB"),

            high_boost: FloatParam::new(
                "High Boost",
                0.0,
                FloatRange::Linear {
                    min: -12.0,
                    max: 12.0,
                },
            )
            .with_smoother(SmoothingStyle::Linear(50.0))
            .with_unit(" dB"),

            oversampling_factor: IntParam::new(
                "Oversampling",
                DEFAULT_OVERSAMPLING_FACTOR as i32,
                IntRange::Linear {
                    min: 0,
                    max: MAX_OVERSAMPLING_FACTOR as i32,
                },
            )
            .with_unit("x")
            .with_value_to_string(Arc::new(|value| {
                let oversampling_times = 2usize.pow(value as u32);
                oversampling_times.to_string()
            }))
            .with_string_to_value(Arc::new(|string| {
                let oversampling_times: usize = string.parse().ok()?;
                Some((oversampling_times as f32).log2() as i32)
            })),
        }
    }
}

impl Plugin for Melter {
    const NAME: &'static str = "Melter";
    const VENDOR: &'static str = "SeedyROM (Zack Kollar)";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "me@seedyrom.io";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            ..AudioIOLayout::const_default()
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            ..AudioIOLayout::const_default()
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        let sample_rate = buffer_config.sample_rate;
        self.sample_rate = sample_rate;

        let num_channels = audio_io_layout
            .main_output_channels
            .expect("Plugin was initialized without any outputs")
            .get() as usize;

        self.parametric_eqs.resize_with(num_channels, || {
            let mut eq = equalization::ParametricEQ::new(sample_rate);

            // Add the bands
            eq.add_band(equalization::BandType::LowShelf, 100.0, 0.0, 1.0)
                .unwrap();
            eq.add_band(equalization::BandType::Peak, 1000.0, 0.0, 1.0)
                .unwrap();
            eq.add_band(equalization::BandType::HighShelf, 10000.0, 0.0, 1.0)
                .unwrap();

            eq
        });

        self.oversamplers.resize_with(num_channels, || {
            oversampling::Lanczos3Oversampler::new(MAX_BLOCK_SIZE, MAX_OVERSAMPLING_FACTOR)
        });

        self.dc_blockers
            .resize_with(num_channels, || filters::DCBlocker::new(sample_rate));

        if let Some(oversampler) = self.oversamplers.first() {
            context.set_latency_samples(
                oversampler.latency(self.params.oversampling_factor.value() as usize),
            );
        }

        true
    }

    fn reset(&mut self) {
        for oversampler in &mut self.oversamplers {
            oversampler.reset();
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let oversampling_factor = self.params.oversampling_factor.value() as usize;
        let oversampling_times = oversampling_factor_to_times(oversampling_factor);

        // If the oversampling factor parameter is changed then the host needs to know about the new
        // latency
        if let Some(oversampler) = self.oversamplers.first() {
            context.set_latency_samples(oversampler.latency(oversampling_factor));
        }

        // Set the sample_rate of the EQs
        for (eq, dc_blocker) in &mut self.parametric_eqs.iter_mut().zip(&mut self.dc_blockers) {
            eq.set_sample_rate(self.sample_rate * oversampling_times as f32);
            dc_blocker.set_sample_rate(self.sample_rate * oversampling_times as f32);
        }

        for (_, block) in buffer.iter_blocks(MAX_BLOCK_SIZE) {
            let block_len = block.samples();
            let upsampled_block_len = block_len * oversampling_times;

            // Get the params for this block
            let pre_post_eq = self.params.pre_post_eq.value();
            let gain = param_next_block!(self, gain, upsampled_block_len);
            let drive = param_next_block!(self, drive, upsampled_block_len);

            // Apply the EQ params
            for (channel_num, block_channel) in block.into_iter().enumerate() {
                let eq = &mut self.parametric_eqs[channel_num];
                let oversampler = &mut self.oversamplers[channel_num];
                let dc_blocker = &mut self.dc_blockers[channel_num];

                // Set the EQ band params
                let low_boost = self.params.low_boost.smoothed.next();
                let mid_boost = self.params.mid_boost.smoothed.next();
                let high_boost = self.params.high_boost.smoothed.next();
                eq.set_band_params(0, 100.0, low_boost, 0.5).unwrap();
                eq.set_band_params(1, 1000.0, mid_boost, 1.0).unwrap();
                eq.set_band_params(2, 10000.0, high_boost, 0.5).unwrap();

                oversampler.process(block_channel, oversampling_factor, |upsampled| {
                    for (sample_idx, sample) in upsampled.iter_mut().enumerate() {
                        // Get the gain and drive for this sample
                        let _gain = gain[sample_idx];
                        let _drive = drive[sample_idx];

                        // Apply the gain
                        *sample *= _gain;

                        // // Apply pre EQ
                        if pre_post_eq {
                            *sample = eq.process(*sample);
                        }

                        // Apply the cubic non-linearity
                        *sample = nonlinearity::cubic(*sample, _drive, 0.5);

                        // Apply the DC blocker, using the this nice magic coefficient!
                        *sample = dc_blocker.process(*sample);

                        // // Apply post EQ
                        if !pre_post_eq {
                            *sample = eq.process(*sample);
                        }
                    }
                });
            }
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for Melter {
    const CLAP_ID: &'static str = "com.seedyrom.melter";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A distortion plugin for fun times!");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for Melter {
    const VST3_CLASS_ID: [u8; 16] = *b"SeedyROMMelter!!";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Distortion];
}

nih_export_clap!(Melter);
nih_export_vst3!(Melter);

// Used in the conversion for the oversampling amount parameter
const fn oversampling_factor_to_times(factor: usize) -> usize {
    2usize.pow(factor as u32)
}
