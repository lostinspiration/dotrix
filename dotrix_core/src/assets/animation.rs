use std::time::Duration;
use std::collections::HashMap;
use super::skin::JointId;

use crate::renderer::transform::TransformBuilder;
use dotrix_math::{ slerp, Vec3, Quat, VectorSpace };

#[derive(Debug)]
pub enum Interpolation {
    Linear,
    Step,
    CubicSpline,
}

impl Interpolation {
    pub fn from(interpolation: gltf::animation::Interpolation) -> Self {
        match interpolation {
            gltf::animation::Interpolation::Linear => Interpolation::Linear,
            gltf::animation::Interpolation::CubicSpline => Interpolation::CubicSpline,
            gltf::animation::Interpolation::Step => Interpolation::Step,
        }
    }
}

trait Interpolate: Copy {
    fn linear(self, target: Self, value: f32) -> Self;
}

impl Interpolate for Vec3{
    fn linear(self, target: Self, value: f32) -> Self {
        self.lerp(target, value)
    }
}

impl Interpolate for Quat {
    fn linear(self, target: Self, value: f32) -> Self {
        slerp(self, target, value)
    }
}

/// Keyframes for the channel transformations of type T
pub struct KeyFrame<T> {
    transformation: T,
    timestamp: f32,
}

impl<T> KeyFrame<T> {
    fn new(timestamp: f32, transformation: T) -> Self {
        Self {
            timestamp,
            transformation
        }
    }
}

struct Channel<T: Interpolate + Copy + Clone> {
    keyframes: Vec<KeyFrame<T>>,
    joint_id: JointId,
    interpolation: Interpolation,
}

impl<T: Interpolate + Copy + Clone> Channel<T> {
    fn from(
        joint_id: JointId,
        interpolation: Interpolation,
        timestamps: Vec<f32>,
        transforms: Vec<T>
    ) -> Self {
        let keyframes = timestamps.into_iter().zip(transforms.into_iter()).map(
            |(timestamp, transformation)| KeyFrame::new(timestamp, transformation)
        ).collect::<Vec<_>>();

        Channel {
            interpolation,
            keyframes,
            joint_id,
        }
    }

    fn sample(&self, keyframe: f32) -> Option<T> {
        for i in 0..self.keyframes.len() - 1 {
            let first = &self.keyframes[i];
            let next = &self.keyframes[i + 1];
            if keyframe >= first.timestamp && keyframe < next.timestamp {
                return match self.interpolation {
                    Interpolation::Step => Some(first.transformation),
                    Interpolation::Linear => {
                        let value = (keyframe - first.timestamp) /
                            (next.timestamp - first.timestamp);
                        Some(first.transformation.linear(next.transformation, value))
                    },
                    _ => panic!("Unsupported interpolation {:?}", self.interpolation),
                };
            }
        }
        None
    }
}

pub struct Animation {
    duration: Duration,
    translation_channels: Vec<Channel<Vec3>>,
    rotation_channels: Vec<Channel<Quat>>,
    scale_channels: Vec<Channel<Vec3>>,
}

impl Animation {
    pub fn new() -> Self {
        Self {
            duration: Duration::from_secs(0),
            translation_channels: Vec::new(),
            rotation_channels: Vec::new(),
            scale_channels: Vec::new(),
        }
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }

    pub fn add_translation_channel(
        &mut self,
        joint_id: JointId,
        interpolation: Interpolation,
        timestamps: Vec<f32>,
        translations: Vec<Vec3>,
    ) {
        self.update_duration(&timestamps);
        self.translation_channels.push(Channel::from(joint_id, interpolation, timestamps, translations));
    }

    pub fn add_rotation_channel(
        &mut self,
        joint_id: JointId,
        interpolation: Interpolation,
        timestamps: Vec<f32>,
        rotations: Vec<Quat>,
    ) {
        self.update_duration(&timestamps);
        self.rotation_channels.push(Channel::from(joint_id, interpolation, timestamps, rotations));
    }

    pub fn add_scale_channel(
        &mut self,
        joint_id: JointId,
        interpolation: Interpolation,
        timestamps: Vec<f32>,
        scales: Vec<Vec3>,
    ) {
        self.update_duration(&timestamps);
        self.scale_channels.push(Channel::from(joint_id, interpolation, timestamps, scales));
    }

    fn update_duration(&mut self, timestamps: &[f32]) {
        let max_timestamp = timestamps.last().copied().unwrap_or(0.0);
        let duration = Duration::from_secs_f32(max_timestamp);
        if duration > self.duration {
            self.duration = duration;
        }
    }

    pub fn sample(&self, keyframe: f32) -> HashMap<JointId, TransformBuilder> {
        let mut result = HashMap::new();

        for channel in &self.translation_channels {
            if let Some(transform) = channel.sample(keyframe) {
                result.insert(channel.joint_id, TransformBuilder::from_translation(transform));
            }
        }

        for channel in &self.rotation_channels {
            if let Some(transform) = channel.sample(keyframe) {
                if let Some(t) = result.get_mut(&channel.joint_id) {
                    t.rotate = Some(transform);
                } else {
                    result.insert(channel.joint_id, TransformBuilder::from_rotation(transform));
                }
            }
        }
        for channel in &self.scale_channels {
            if let Some(transform) = channel.sample(keyframe) {
                if let Some(t) = result.get_mut(&channel.joint_id) {
                    t.scale = Some(transform);
                } else {
                    result.insert(channel.joint_id, TransformBuilder::from_scale(transform));
                }
            }
        }

        result
    }
}

impl Default for Animation {
    fn default() -> Self {
        Self::new()
    }
}
