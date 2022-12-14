use alloc::vec::Vec;
use core::fmt;

use bitvec::prelude::*;
pub use uom::si;
pub use uom::si::f64::{
    Acceleration, AngularVelocity, ElectricCurrent, ElectricPotential, Length, Time, Velocity,
};

use crate::headers::FirmwareKind;
use crate::Headers;

#[allow(unreachable_pub)]
pub(crate) mod prelude {
    pub use super::si::acceleration::meter_per_second_squared as mps2;
    pub use super::si::angular_velocity::degree_per_second;
    pub use super::si::electric_current::{ampere, milliampere};
    pub use super::si::electric_potential::{millivolt, volt};
    pub use super::si::length::meter;
    pub use super::si::time::{microsecond, second};
    pub use super::si::velocity::meter_per_second;
    pub use super::{
        Acceleration, AngularVelocity, ElectricCurrent, ElectricPotential, Length, Time, Velocity,
    };
}

mod from_raw {
    #[allow(unreachable_pub)]
    pub trait FromRaw {
        type Raw;
        fn from_raw(raw: Self::Raw, headers: &super::Headers) -> Self;
    }
}

pub(crate) use from_raw::FromRaw;

impl FromRaw for Time {
    type Raw = u64;

    fn from_raw(raw: Self::Raw, _headers: &Headers) -> Self {
        Self::new::<prelude::microsecond>(raw as f64)
    }
}

impl FromRaw for Acceleration {
    type Raw = i32;

    fn from_raw(raw: Self::Raw, headers: &Headers) -> Self {
        // TODO: switch to `standard_gravity` instead of `mps2` once
        // https://github.com/iliekturtles/uom/pull/351 lands

        let gs = f64::from(raw) / f64::from(headers.acceleration_1g.unwrap());
        Self::new::<prelude::mps2>(gs * 9.80665)
    }
}

impl FromRaw for AngularVelocity {
    type Raw = i32;

    fn from_raw(raw: Self::Raw, headers: &Headers) -> Self {
        let scale = headers.gyro_scale.unwrap();
        let rad = f64::from(scale) * f64::from(raw);

        AngularVelocity::new::<si::angular_velocity::radian_per_second>(rad)
    }
}

impl FromRaw for ElectricCurrent {
    type Raw = i32;

    fn from_raw(raw: Self::Raw, _headers: &Headers) -> Self {
        new_amps(raw)
    }
}

/// Correct from BF 3.1.7 (3.1.0?), INAV 2.0.0
#[inline(always)]
fn new_amps(raw: i32) -> ElectricCurrent {
    ElectricCurrent::new::<si::electric_current::centiampere>(raw.into())
}

impl FromRaw for ElectricPotential {
    type Raw = u32;

    fn from_raw(raw: Self::Raw, _headers: &Headers) -> Self {
        new_vbat(raw)
    }
}

/// Correct from BF 4.0.0, INAV 3.0.0?
#[inline(always)]
fn new_vbat(raw: u32) -> ElectricPotential {
    ElectricPotential::new::<si::electric_potential::centivolt>(raw.into())
}

impl FromRaw for Velocity {
    type Raw = u32;

    fn from_raw(raw: Self::Raw, _headers: &Headers) -> Self {
        Self::new::<si::velocity::centimeter_per_second>(raw.into())
    }
}

pub trait FlagSet {
    type Flag: Flag;

    /// Checks if a given flag is enabled.
    fn is_set(&self, flag: Self::Flag) -> bool;

    /// Returns the names of all enabled flags.
    fn as_names(&self) -> Vec<&'static str>;
}

pub trait Flag {
    /// Returns the name of this flag.
    fn as_name(&self) -> &'static str;
}

macro_rules! define_flag_set {
    ($(#[$set_attr:meta])* $set:ident, $(#[$flag_attr:meta])* $flag_name:ident {
        $( $flag:ident : $($beta:literal)? / $($inav:literal)? ),* $(,)?
    }) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $(#[$set_attr])*
        pub struct $set {
            firmware: FirmwareKind,
            raw: BitArray<[u32; 1], Lsb0>,
        }

        impl $set {
            pub(crate) fn new(raw: u32, firmware: FirmwareKind) -> Self {
                Self {
                    firmware,
                    raw: BitArray::new([raw]),
                }
            }
        }

        impl FlagSet for $set {
            type Flag = $flag_name;

            fn is_set(&self, flag: Self::Flag) -> bool {
                flag.to_bit(self.firmware)
                    .map_or(false, |bit| self.raw[bit])
            }

            fn as_names(&self) -> Vec<&'static str> {
                self.raw
                    .iter_ones()
                    .filter_map(|bit| Some($flag_name::from_bit(bit, self.firmware)?.as_name()))
                    .collect()
            }
        }

        impl fmt::Display for $set {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.as_names().join("|"))
            }
        }

        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        $(#[$flag_attr])*
        pub enum $flag_name {
            $( $flag ),*
        }

        impl Flag for $flag_name {
            fn as_name(&self) -> &'static str {
                match self {
                    $( Self::$flag => stringify!($flag) ),*
                }
            }
        }

        impl $flag_name {
            const fn from_bit(bit: usize, firmware: FirmwareKind) -> Option<Self> {
                match (bit, firmware) {
                    $($( ($beta, FirmwareKind::Betaflight | FirmwareKind::EmuFlight) => Some(Self::$flag), )?)*
                    $($( ($inav, FirmwareKind::Inav) => Some(Self::$flag), )?)*
                    _ => None,
                }
            }

            const fn to_bit(self, firmware: FirmwareKind) -> Option<usize> {
                match (self, firmware) {
                    $($( (Self::$flag, FirmwareKind::Betaflight | FirmwareKind::EmuFlight) => Some($beta), )?)*
                    $($( (Self::$flag, FirmwareKind::Inav) => Some($inav), )?)*
                    _ => None,
                }
            }
        }

        impl fmt::Display for $flag_name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_name())
            }
        }
    };
}

define_flag_set! {
    /// All currently enabled flight modes. See [`FlagSet`] and [`FlightMode`].
    FlightModeSet,
    /// A flight mode. See [`Flag`].
    FlightMode {
        Angle:     1 /  0,
        Horizon:   2 /  1,
        HeadFree:  4 /  6,
        Failsafe:  6 /  9,
        Turtle:   27 / 15,

        Arm:            0 / ,
        Mag:            3 / ,
        Passthru:       5 / ,
        GpsRescue:      7 / ,
        Antigravity:    8 / ,
        HeadAdjust:     9 / ,
        CamStab:       10 / ,
        BeeperOn:      11 / ,
        LedLow:        12 / ,
        Calib:         13 / ,
        Osd:           14 / ,
        Telemetry:     15 / ,
        Servo1:        16 / ,
        Servo2:        17 / ,
        Servo3:        18 / ,
        Blackbox:      19 / ,
        Airmode:       20 / ,
        ThreeD:        21 / ,
        FpvAngleMix:   22 / ,
        BlackboxErase: 23 / ,
        Camera1:       24 / ,
        Camera2:       25 / ,
        Camera3:       26 / ,
        Prearm:        28 / ,
        BeepGpsCount:  29 / ,
        VtxPitmode:    30 / ,
        Paralyze:      31 / ,

        // User1:               32 / ,
        // User2:               33 / ,
        // User3:               34 / ,
        // User4:               35 / ,
        // PidAudio:            36 / ,
        // AcroTrainer:         37 / ,
        // VtxControlDisable:   38 / ,
        // LaunchControl:       39 / ,
        // MspOverride:         40 / ,
        // StickCommandDisable: 41 / ,
        // BeeperMute:          42 / ,

        Heading:       /  2,
        NavAltHold:    /  3,
        NavRth:        /  4,
        NavPoshold:    /  5,
        NavLaunch:     /  7,
        Manual:        /  8,
        AutoTune:      / 10,
        NavWp:         / 11,
        NavCourseHold: / 12,
        Flaperon:      / 13,
        TurnAssistant: / 14,
        Soaring:       / 16,
    }
}

define_flag_set! {
    /// All currently enabled states. See [`FlagSet`] and [`State`].
    StateSet,
    /// A flight controller state. See [`Flag`].
    State {
        GpsFixHome: 0 / 0,
        GpsFix:     1 / 1,
        GpsFixEver: 2 /  ,

        CalibrateMag:                 /  2,
        SmallAngle:                   /  3,
        FixedWingLegacy:              /  4,
        AntiWindup:                   /  5,
        FlaperonAvailable:            /  6,
        NavMotorStopOrIdle:           /  7,
        CompassCalibrated:            /  8,
        AccelerometerCalibrated:      /  9,
        NavCruiseBraking:             / 11,
        NavCruiseBrakingBoost:        / 12,
        NavCruiseBrakingLocked:       / 13,
        NavExtraArmingSafetyBypassed: / 14,
        AirmodeActive:                / 15,
        EscSensorEnabled:             / 16,
        Airplane:                     / 17,
        Multirotor:                   / 18,
        Rover:                        / 19,
        Boat:                         / 20,
        AltitudeControl:              / 21,
        MoveForwardOnly:              / 22,
        SetReversibleMotorsForward:   / 23,
        FwHeadingUseYaw:              / 24,
        AntiWindupDeactivated:        / 25,
        LandingDetected:              / 26,
    }
}

/// The current failsafe phase. See [`Flag`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailsafePhase {
    Idle,
    RxLossDetected,
    RxLossIdle,
    ReturnToHome,
    Landing,
    Landed,
    RxLossMonitoring,
    RxLossRecovered,
    GpsRescue,
    Unknown,
}

impl FailsafePhase {
    pub(crate) fn new(raw: u32, firmware: FirmwareKind) -> Self {
        let mapping: &[Self] = if firmware == FirmwareKind::Inav {
            &[
                Self::Idle,
                Self::RxLossDetected,
                Self::RxLossIdle,
                Self::ReturnToHome,
                Self::Landing,
                Self::Landed,
                Self::RxLossMonitoring,
                Self::RxLossRecovered,
            ]
        } else {
            &[
                Self::Idle,
                Self::RxLossDetected,
                Self::Landing,
                Self::Landed,
                Self::RxLossMonitoring,
                Self::RxLossRecovered,
                Self::GpsRescue,
            ]
        };

        usize::try_from(raw)
            .ok()
            .and_then(|index| mapping.get(index))
            .copied()
            .unwrap_or_else(|| {
                tracing::debug!("invalid failsafe phase ({raw})");
                Self::Unknown
            })
    }
}

impl Flag for FailsafePhase {
    fn as_name(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::RxLossDetected => "RxLossDetected",
            Self::RxLossIdle => "RxLossIdle",
            Self::ReturnToHome => "ReturnToHome",
            Self::Landing => "Landing",
            Self::Landed => "Landed",
            Self::RxLossMonitoring => "RxLossMonitoring",
            Self::RxLossRecovered => "RxLossRecovered",
            Self::GpsRescue => "GpsRescue",
            Self::Unknown => "Unknown",
        }
    }
}

impl fmt::Display for FailsafePhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    macro_rules! float_eq {
        ($left:expr, $right:expr) => {
            let epsilon = 0.0001;
            let diff = ($left - $right).abs();
            assert!(
                diff < epsilon,
                "{left} and {right} are greater than {epsilon} apart: {diff}",
                left = $left,
                right = $right
            );
        };
    }

    #[test]
    fn electric_current() {
        float_eq!(1.39, new_amps(139).get::<prelude::ampere>());
    }

    #[test]
    fn electric_potential() {
        float_eq!(16.32, new_vbat(1632).get::<prelude::volt>());
    }

    mod resolution {
        use super::*;

        #[test]
        fn time() {
            use si::time::{day, microsecond};

            let ms = Time::new::<microsecond>(1.);
            float_eq!(1., ms.get::<microsecond>());

            let d = Time::new::<day>(1.);
            float_eq!(1., d.get::<day>());

            float_eq!(
                ms.get::<microsecond>() + d.get::<microsecond>(),
                (ms + d).get::<microsecond>()
            );
        }

        #[test]
        fn acceleration() {
            use si::acceleration::{
                kilometer_per_second_squared as kmps2, millimeter_per_second_squared as mmps2,
            };

            let mm = Acceleration::new::<mmps2>(1.);
            float_eq!(1., mm.get::<mmps2>());

            let km = Acceleration::new::<kmps2>(1.);
            float_eq!(1., km.get::<kmps2>());

            float_eq!(
                mm.get::<mmps2>() + km.get::<mmps2>(),
                (mm + km).get::<mmps2>()
            );
        }

        #[test]
        fn angular_velocity() {
            use si::angular_velocity::degree_per_second as dps;

            let slow = AngularVelocity::new::<dps>(0.01);
            float_eq!(0.01, slow.get::<dps>());

            let fast = AngularVelocity::new::<dps>(5_000.);
            float_eq!(5_000., fast.get::<dps>());

            float_eq!(5_000.01, (slow + fast).get::<dps>());
        }

        #[test]
        fn electric_current() {
            use si::electric_current::{kiloampere, milliampere};

            let ma = ElectricCurrent::new::<milliampere>(1.);
            float_eq!(1., ma.get::<milliampere>());

            let ka = ElectricCurrent::new::<kiloampere>(1.);
            float_eq!(1., ka.get::<kiloampere>());

            float_eq!(
                ma.get::<milliampere>() + ka.get::<milliampere>(),
                (ma + ka).get::<milliampere>()
            );
        }

        #[test]
        fn electric_potential() {
            use si::electric_potential::{kilovolt, millivolt};

            let mv = ElectricPotential::new::<millivolt>(1.);
            float_eq!(1., mv.get::<millivolt>());

            let kv = ElectricPotential::new::<kilovolt>(1.);
            float_eq!(1., kv.get::<kilovolt>());

            float_eq!(
                mv.get::<millivolt>() + kv.get::<millivolt>(),
                (mv + kv).get::<millivolt>()
            );
        }
    }
}
