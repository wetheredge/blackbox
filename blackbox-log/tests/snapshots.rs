use std::collections::BTreeMap;
use std::fs;
use std::io::Read;

use blackbox_log::parser::{Event, Headers, Stats, Unit, Value};
use blackbox_log::units::{si, FlagSet};
use blackbox_log::Log;
use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;

macro_rules! run {
    () => {
        |path| {
            let mut file = fs::File::open(path).unwrap();
            let mut data = Vec::new();
            file.read_to_end(&mut data).unwrap();

            let file = blackbox_log::File::new(&data);
            let logs = file
                .parse_iter()
                .map(|r| r.map(LogSnapshot::from))
                .collect::<Vec<_>>();

            insta::assert_ron_snapshot!(logs);
        }
    };
}

#[test]
fn own() {
    insta::glob!("logs/*.bbl", run!());
}

#[test]
#[ignore]
fn fc_blackbox() {
    insta::glob!("logs/fc-blackbox/*", run!());
}

#[test]
#[ignore]
fn gimbal_ghost() {
    insta::glob!("logs/gimbal-ghost/*", run!());
}

#[derive(Debug, Serialize)]
struct LogSnapshot<'a> {
    headers: Headers<'a>,
    stats: Stats,
    events: Vec<Event>,
    fields: Fields,
}

impl<'a> From<Log<'a>> for LogSnapshot<'a> {
    fn from(log: Log<'a>) -> Self {
        let fields = log.iter_fields().collect::<Fields>();

        let fields = log.iter_frames().fold(fields, |mut fields, frame| {
            fields.update(frame);
            fields
        });

        Self {
            headers: log.headers().clone(),
            stats: log.stats(),
            events: log.events().to_owned(),
            fields,
        }
    }
}

#[derive(Debug, Serialize)]
struct Fields(Vec<FieldSnapshot>);

impl Fields {
    fn update(&mut self, frame: impl Iterator<Item = Value>) {
        for (field, value) in self.0.iter_mut().zip(frame) {
            field.update(value);
        }
    }
}

impl<T> FromIterator<(T, Unit)> for Fields
where
    T: Into<String>,
{
    fn from_iter<I: IntoIterator<Item = (T, Unit)>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|(name, unit)| FieldSnapshot::new(name.into(), unit))
                .collect(),
        )
    }
}

#[derive(Debug, Serialize)]
struct FieldSnapshot {
    name: String,
    unit: Unit,
    history: History,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum History {
    Int(NumberHistory<16, i128>),
    Bool { yes: usize, no: usize },
    Flags(BTreeMap<&'static str, usize>),
}

#[derive(Debug)]
struct NumberHistory<const N: usize, T> {
    min: T,
    max: T,
    seen: Vec<T>,
    histogram: [usize; N],
}

impl FieldSnapshot {
    fn new(name: String, unit: Unit) -> Self {
        Self {
            name,
            unit,
            history: match unit {
                Unit::FrameTime
                | Unit::Amperage
                | Unit::Voltage
                | Unit::Acceleration
                | Unit::Rotation
                | Unit::Unitless => History::Int(NumberHistory::new()),
                Unit::Boolean => History::Bool { yes: 0, no: 0 },
                Unit::FlightMode | Unit::State | Unit::FailsafePhase => {
                    History::Flags(BTreeMap::new())
                }
            },
        }
    }

    #[allow(clippy::wildcard_enum_match_arm, clippy::cast_possible_truncation)]
    fn update(&mut self, value: Value) {
        match &mut self.history {
            History::Int(history) => history.update(match value {
                Value::FrameTime(t) => t.get::<si::time::microsecond>().into(),
                Value::Amperage(a) => a.get::<si::electric_current::milliampere>().round() as i128,
                Value::Voltage(v) => v.get::<si::electric_potential::millivolt>().round() as i128,
                Value::Acceleration(a) => a
                    .get::<si::acceleration::centimeter_per_second_squared>()
                    .round() as i128,
                Value::Rotation(r) => {
                    r.get::<si::angular_velocity::degree_per_second>().round() as i128
                }
                Value::Unsigned(u) => u.into(),
                Value::Signed(s) => s.into(),
                _ => unreachable!(),
            }),
            History::Bool { yes, no } => match value {
                Value::Boolean(true) => *yes += 1,
                Value::Boolean(false) => *no += 1,
                _ => unreachable!(),
            },
            History::Flags(history) => {
                let flags = match value {
                    Value::FlightMode(m) => m.as_names(),
                    Value::State(s) => s.as_names(),
                    Value::FailsafePhase(f) => f.as_names(),
                    _ => unreachable!(),
                };

                for flag in flags {
                    *history.entry(flag).or_insert(0) += 1;
                }
            }
        }
    }
}

impl<const N: usize, T> NumberHistory<N, T> {
    fn new() -> Self
    where
        T: Default,
    {
        Self {
            min: T::default(),
            max: T::default(),
            seen: Vec::new(),
            histogram: [0; N],
        }
    }

    fn update_range(&mut self, value: T)
    where
        T: PartialOrd,
    {
        if value < self.min {
            self.min = value;
        } else if value > self.max {
            self.max = value;
        }
    }

    /// Store new value sorted in `seen` if not already present
    fn update_seen(&mut self, value: T)
    where
        T: PartialOrd,
    {
        let index = self.seen.partition_point(|x| *x < value);
        if self.seen.get(index) != Some(&value) {
            self.seen.insert(index, value);
        }
    }

    /// Bucket so that sequential values go in sequential buckets, since data is
    /// usually clustered
    #[inline(always)]
    fn update_histogram(&mut self, value: u128) {
        self.histogram[(value % N as u128) as usize] += 1;
    }
}

impl<const N: usize> NumberHistory<N, i128> {
    fn update(&mut self, value: i128) {
        self.update_range(value);
        self.update_seen(value);
        self.update_histogram(value.unsigned_abs());
    }
}

impl<const N: usize> Serialize for NumberHistory<N, i128>
where
    [usize; N]: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(self, self.min, self.max, serializer)
    }
}

impl<const N: usize> Serialize for NumberHistory<N, f64>
where
    [usize; N]: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize(
            self,
            format!("{:.2}", self.min),
            format!("{:.2}", self.max),
            serializer,
        )
    }
}

#[inline(always)]
fn serialize<const N: usize, T, S, U>(
    snapshot: &NumberHistory<N, T>,
    min: U,
    max: U,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: Serializer,
    U: Serialize,
    [usize; N]: Serialize,
{
    let unique = snapshot.seen.len();

    let mut state = serializer.serialize_struct("FieldSnapshot", 4)?;

    state.serialize_field("min", &min)?;
    state.serialize_field("max", &max)?;
    state.serialize_field("unique", &unique)?;

    let histogram = "histogram";
    if unique > 1 {
        state.serialize_field(histogram, &snapshot.histogram)?;
    } else {
        state.skip_field(histogram)?;
    }

    state.end()
}
