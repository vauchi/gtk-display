// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

use pipewire::{self as pw, spa};
use spa::param::{
    ParamType,
    format::{FormatProperties as F, MediaSubtype, MediaType},
    video::VideoFormat,
};
use spa::pod::{Object, Pod, Property, PropertyFlags, Value};
use spa::utils::{Choice, ChoiceEnum, ChoiceFlags, SpaTypes};

fn serialize(value: Value) -> Option<Vec<u8>> {
    Some(
        spa::pod::serialize::PodSerializer::serialize(std::io::Cursor::new(Vec::new()), &value)
            .ok()?
            .0
            .into_inner(),
    )
}

pub(super) fn video() -> Option<Vec<u8>> {
    serialize(Value::Object(spa::pod::object!(
        SpaTypes::ObjectParamFormat,
        ParamType::EnumFormat,
        spa::pod::property!(F::MediaType, Id, MediaType::Video),
        spa::pod::property!(F::MediaSubtype, Id, MediaSubtype::Raw),
        spa::pod::property!(F::VideoFormat, Id, VideoFormat::RGBA),
        spa::pod::property!(
            F::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 640,
                height: 480
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 1920,
                height: 1080
            }
        ),
    )))
}

pub(super) fn buffers(stream: &pw::stream::Stream, width: u32, height: u32) -> Option<()> {
    let mask = (1 << spa::sys::SPA_DATA_MemPtr) | (1 << spa::sys::SPA_DATA_MemFd);
    let property = |key, value| Property {
        key,
        flags: PropertyFlags::empty(),
        value,
    };
    let bytes = serialize(Value::Object(Object {
        type_: SpaTypes::ObjectParamBuffers.as_raw(),
        id: ParamType::Buffers.as_raw(),
        properties: vec![
            property(
                spa::sys::SPA_PARAM_BUFFERS_dataType,
                Value::Choice(spa::pod::ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Flags {
                        default: mask,
                        flags: vec![mask],
                    },
                ))),
            ),
            property(spa::sys::SPA_PARAM_BUFFERS_blocks, Value::Int(1)),
            property(
                spa::sys::SPA_PARAM_BUFFERS_size,
                Value::Int((width * height * 4) as i32),
            ),
            property(
                spa::sys::SPA_PARAM_BUFFERS_stride,
                Value::Int((width * 4) as i32),
            ),
        ],
    }))?;
    stream.update_params(&mut [Pod::from_bytes(&bytes)?]).ok()?;
    Some(())
}
