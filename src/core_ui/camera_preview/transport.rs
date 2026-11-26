// SPDX-FileCopyrightText: 2026 Mattia Egloff <mattia.egloff@pm.me>
// SPDX-License-Identifier: GPL-3.0-or-later

mod formats;

use super::{PreviewBinding, PreviewFrame};
use pipewire::{self as pw, properties::properties, spa};
use pw::stream::{StreamFlags, StreamListener, StreamRc, StreamState};
use spa::{
    param::{
        ParamType,
        video::{VideoFormat, VideoInfoRaw},
    },
    pod::Pod,
    utils::Direction,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

pub(super) struct PreviewStream {
    _registry_events: pw::registry::Listener,
    connected: Option<Connected>,
    _registry: pw::registry::RegistryRc,
    core: pw::core::CoreRc,
    mainloop: pw::main_loop::MainLoopRc,
    binding: Rc<RefCell<PreviewBinding>>,
    latest: Rc<RefCell<Option<PreviewFrame>>>,
    failed: Rc<Cell<bool>>,
}

struct Connected {
    _events: StreamListener<VideoInfoRaw>,
    _stream: StreamRc,
}

impl PreviewStream {
    pub(super) fn new(preview_id: &str) -> Option<Self> {
        let binding = Rc::new(RefCell::new(PreviewBinding::new(preview_id)?));
        pw::init();
        let mainloop = pw::main_loop::MainLoopRc::new(None).ok()?;
        let context = pw::context::ContextRc::new(&mainloop, None).ok()?;
        let core = context.connect_rc(None).ok()?;
        let registry = core.get_registry_rc().ok()?;
        let observed = binding.clone();
        let removed = binding.clone();
        let _registry_events = registry
            .add_listener_local()
            .global(move |object| {
                let Some(props) = object.props else {
                    return;
                };
                if let (Some(name), Some(class), Some(serial)) = (
                    props.get("node.name"),
                    props.get("media.class"),
                    props.get("object.serial"),
                ) {
                    observed
                        .borrow_mut()
                        .observe(object.id, name, class, serial);
                }
            })
            .global_remove(move |id| removed.borrow_mut().remove(id))
            .register();
        Some(Self {
            _registry_events,
            connected: None,
            _registry: registry,
            core,
            mainloop,
            binding,
            latest: Rc::new(RefCell::new(None)),
            failed: Rc::new(Cell::new(false)),
        })
    }

    pub(super) fn poll(&mut self) -> Option<PreviewFrame> {
        self.mainloop.loop_().iterate(pw::loop_::Timeout::None);
        if self.closed() {
            self.connected = None;
            self.latest.borrow_mut().take();
            return None;
        }
        let target = self
            .binding
            .borrow()
            .current()
            .map(|(id, serial)| (id, serial.to_owned()));
        if self.connected.is_none()
            && let Some((id, serial)) = target
        {
            self.connected = Connected::new(
                self.core.clone(),
                id,
                serial,
                self.latest.clone(),
                self.failed.clone(),
            );
            if self.connected.is_none() {
                self.failed.set(true);
            }
        }
        self.latest.borrow_mut().take()
    }

    pub(super) fn closed(&self) -> bool {
        self.failed.get() || self.binding.borrow().retired()
    }
}

impl Connected {
    fn new(
        core: pw::core::CoreRc,
        id: u32,
        serial: String,
        latest: Rc<RefCell<Option<PreviewFrame>>>,
        failed: Rc<Cell<bool>>,
    ) -> Option<Self> {
        let stream = StreamRc::new(
            core,
            "Vauchi GTK camera preview",
            properties! {
                "media.type" => "Video", "media.category" => "Capture", "media.role" => "Camera",
                "target.object" => serial,
            },
        )
        .ok()?;
        let state_failed = failed.clone();
        let params_failed = failed;
        let events = stream
            .add_local_listener_with_user_data(VideoInfoRaw::default())
            .state_changed(move |_, _, _, next| {
                if matches!(next, StreamState::Error(_)) {
                    state_failed.set(true);
                }
            })
            .param_changed(move |stream, format, id, param| {
                if id != ParamType::Format.as_raw() {
                    return;
                }
                let Some(param) = param else {
                    return;
                };
                if format.parse(param).is_err()
                    || format.format() != VideoFormat::RGBA
                    || format.size().width == 0
                    || format.size().height == 0
                    || format.size().width > 1920
                    || format.size().height > 1080
                    || formats::buffers(stream, format.size().width, format.size().height).is_none()
                {
                    params_failed.set(true);
                }
            })
            .process(move |stream, format| {
                let Some(mut buffer) = stream.dequeue_buffer() else {
                    return;
                };
                let Some(data) = buffer.datas_mut().first_mut() else {
                    return;
                };
                let offset = data.chunk().offset() as usize;
                let Some(end) = offset.checked_add(data.chunk().size() as usize) else {
                    return;
                };
                let Ok(stride) = usize::try_from(data.chunk().stride()) else {
                    return;
                };
                let Some(bytes) = data.data().and_then(|bytes| bytes.get(offset..end)) else {
                    return;
                };
                *latest.borrow_mut() = PreviewFrame::from_rgba(
                    format.size().width,
                    format.size().height,
                    stride,
                    bytes,
                );
            })
            .register()
            .ok()?;
        let format = formats::video()?;
        stream
            .connect(
                Direction::Input,
                Some(id),
                StreamFlags::AUTOCONNECT | StreamFlags::MAP_BUFFERS | StreamFlags::DONT_RECONNECT,
                &mut [Pod::from_bytes(&format)?],
            )
            .ok()?;
        Some(Self {
            _events: events,
            _stream: stream,
        })
    }
}
