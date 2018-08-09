/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use euclid::{TypedPoint2D, TypedVector2D};
use glutin_app::keyutils::{CMD_OR_CONTROL};
use glutin_app::window::{Window, LINE_HEIGHT};
use servo::compositing::windowing::WindowEvent;
use servo::embedder_traits::{EmbedderMsg, FilterPattern};
use servo::msg::constellation_msg::{Key, TopLevelBrowsingContextId as BrowserId};
use servo::msg::constellation_msg::{KeyModifiers, KeyState};
use servo::script_traits::TouchEventType;
use servo::servo_config::opts;
use servo::servo_url::ServoUrl;
use servo::webrender_api::ScrollLocation;
use std::mem;
use std::rc::Rc;
use std::thread;
use tinyfiledialogs::{self, MessageBoxIcon};

pub struct Browser {
    /// id of the top level browsing context. It is unique as tabs
    /// are not supported yet. None until created.
    browser_id: Option<BrowserId>,

    // A rudimentary stack of "tabs".
    // EmbedderMsg::BrowserCreated will push onto it.
    // EmbedderMsg::CloseBrowser will pop from it,
    // and exit if it is empty afterwards.
    browsers: Vec<BrowserId>,

    title: Option<String>,
    status: Option<String>,
    favicon: Option<ServoUrl>,
    loading_state: Option<LoadingState>,
    window: Rc<Window>,
    event_queue: Vec<WindowEvent>,
    shutdown_requested: bool,
}

enum LoadingState {
    Connecting,
    Loading,
    Loaded,
}

impl Browser {
    pub fn new(window: Rc<Window>) -> Browser {
        Browser {
            title: None,
            browser_id: None,
            browsers: Vec::new(),
            status: None,
            favicon: None,
            loading_state: None,
            window,
            event_queue: Vec::new(),
            shutdown_requested: false,
        }
    }

    pub fn get_events(&mut self) -> Vec<WindowEvent> {
        mem::replace(&mut self.event_queue, Vec::new())
    }

    pub fn handle_window_events(&mut self, events: Vec<WindowEvent>) {
        for event in events {
            match event {
                WindowEvent::KeyEvent(ch, key, state, mods) => {
                    self.event_queue.push(WindowEvent::KeyEvent(ch, key, state, mods));
                },
                event => {
                    self.event_queue.push(event);
                }
            }
        }
    }

    pub fn shutdown_requested(&self) -> bool {
        self.shutdown_requested
    }

    /// Handle key events after they have been handled by Servo.
    fn handle_key_from_servo(&mut self, _: Option<BrowserId>, ch: Option<char>,
                             key: Key, state: KeyState, mods: KeyModifiers) {
        if state == KeyState::Pressed {
            return;
        }
        match (mods, ch, key) {
            (_, Some('+'), _) => {
                if mods & !KeyModifiers::SHIFT == CMD_OR_CONTROL {
                    self.event_queue.push(WindowEvent::Zoom(1.1));
                } else if mods & !KeyModifiers::SHIFT == CMD_OR_CONTROL | KeyModifiers::ALT {
                    self.event_queue.push(WindowEvent::PinchZoom(1.1));
                }
            }
            (CMD_OR_CONTROL, Some('-'), _) => {
                self.event_queue.push(WindowEvent::Zoom(1.0 / 1.1));
            }
            (_, Some('-'), _) if mods == CMD_OR_CONTROL | KeyModifiers::ALT => {
                self.event_queue.push(WindowEvent::PinchZoom(1.0 / 1.1));
            }
            (CMD_OR_CONTROL, Some('0'), _) => {
                self.event_queue.push(WindowEvent::ResetZoom);
            }

            (KeyModifiers::NONE, None, Key::PageDown) => {
               let scroll_location = ScrollLocation::Delta(TypedVector2D::new(0.0,
                                   -self.window.page_height() + 2.0 * LINE_HEIGHT));
                self.scroll_window_from_key(scroll_location, TouchEventType::Move);
            }
            (KeyModifiers::NONE, None, Key::PageUp) => {
                let scroll_location = ScrollLocation::Delta(TypedVector2D::new(0.0,
                                   self.window.page_height() - 2.0 * LINE_HEIGHT));
                self.scroll_window_from_key(scroll_location, TouchEventType::Move);
            }

            (KeyModifiers::NONE, None, Key::Home) => {
                self.scroll_window_from_key(ScrollLocation::Start, TouchEventType::Move);
            }

            (KeyModifiers::NONE, None, Key::End) => {
                self.scroll_window_from_key(ScrollLocation::End, TouchEventType::Move);
            }

            (KeyModifiers::NONE, None, Key::Up) => {
                self.scroll_window_from_key(ScrollLocation::Delta(TypedVector2D::new(0.0, 3.0 * LINE_HEIGHT)),
                                            TouchEventType::Move);
            }
            (KeyModifiers::NONE, None, Key::Down) => {
                self.scroll_window_from_key(ScrollLocation::Delta(TypedVector2D::new(0.0, -3.0 * LINE_HEIGHT)),
                                            TouchEventType::Move);
            }
            (KeyModifiers::NONE, None, Key::Left) => {
                self.scroll_window_from_key(ScrollLocation::Delta(TypedVector2D::new(LINE_HEIGHT, 0.0)),
                                            TouchEventType::Move);
            }
            (KeyModifiers::NONE, None, Key::Right) => {
                self.scroll_window_from_key(ScrollLocation::Delta(TypedVector2D::new(-LINE_HEIGHT, 0.0)),
                                            TouchEventType::Move);
            }

            _ => {
            }
        }
    }

    fn scroll_window_from_key(&mut self, scroll_location: ScrollLocation, phase: TouchEventType) {
        let event = WindowEvent::Scroll(scroll_location, TypedPoint2D::zero(), phase);
        self.event_queue.push(event);
    }

    pub fn handle_servo_events(&mut self, events: Vec<(Option<BrowserId>, EmbedderMsg)>) {
        for (browser_id, msg) in events {
            match msg {
                EmbedderMsg::Status(status) => {
                    self.status = status;
                },
                EmbedderMsg::ChangePageTitle(title) => {
                    self.title = title;

                    let fallback_title = String::from("Untitled");
                    let title = match self.title {
                        Some(ref title) if title.len() > 0 => &**title,
                        _ => &fallback_title,
                    };
                    let title = format!("{} - Servo", title);
                    self.window.set_title(&title);
                }
                EmbedderMsg::MoveTo(point) => {
                    self.window.set_position(point);
                }
                EmbedderMsg::ResizeTo(size) => {
                    self.window.set_inner_size(size);
                }
                EmbedderMsg::Alert(message, sender) => {
                    let _ = thread::Builder::new().name("display alert dialog".to_owned()).spawn(move || {
                        tinyfiledialogs::message_box_ok("Alert!", &message, MessageBoxIcon::Warning);
                    }).unwrap().join().expect("Thread spawning failed");
                    if let Err(e) = sender.send(()) {
                        let reason = format!("Failed to send Alert response: {}", e);
                        self.event_queue.push(WindowEvent::SendError(browser_id, reason));
                    }
                }
                EmbedderMsg::AllowUnload(sender) => {
                    let _ = sender.send(false);
                }
                EmbedderMsg::AllowNavigation(_url, sender) => {
                    let _ = sender.send(false);
                }
                EmbedderMsg::AllowOpeningBrowser(response_chan) => {
                    let _ = response_chan.send(false);
                }
                EmbedderMsg::BrowserCreated(new_browser_id) => {
                    // TODO: properly handle a new "tab"
                    self.browsers.push(new_browser_id);
                    if self.browser_id.is_none() {
                        self.browser_id = Some(new_browser_id);
                    }
                    self.event_queue.push(WindowEvent::SelectBrowser(new_browser_id));
                }
                EmbedderMsg::KeyEvent(ch, key, state, modified) => {
                    self.handle_key_from_servo(browser_id, ch, key, state, modified);
                }
                EmbedderMsg::SetCursor(cursor) => {
                    self.window.set_cursor(cursor);
                }
                EmbedderMsg::NewFavicon(url) => {
                    self.favicon = Some(url);
                }
                EmbedderMsg::HeadParsed => {
                    self.loading_state = Some(LoadingState::Loading);
                }
                EmbedderMsg::HistoryChanged(_urls, _current) => {
                }
                EmbedderMsg::SetFullscreenState(state) => {
                    self.window.set_fullscreen(state);
                }
                EmbedderMsg::LoadStart => {
                    self.loading_state = Some(LoadingState::Connecting);
                }
                EmbedderMsg::LoadComplete => {
                    self.loading_state = Some(LoadingState::Loaded);
                }
                EmbedderMsg::CloseBrowser => {
                    // TODO: close the appropriate "tab".
                    let _ = self.browsers.pop();
                    if let Some(prev_browser_id) = self.browsers.last() {
                        self.browser_id = Some(*prev_browser_id);
                        self.event_queue.push(WindowEvent::SelectBrowser(*prev_browser_id));
                    } else {
                        self.event_queue.push(WindowEvent::Quit);
                    }
                },
                EmbedderMsg::Shutdown => {
                    self.shutdown_requested = true;
                },
                EmbedderMsg::Panic(_reason, _backtrace) => {
                },
                EmbedderMsg::GetSelectedBluetoothDevice(_devices, _sender) => {
                },
                EmbedderMsg::SelectFiles(patterns, multiple_files, sender) => {
                    let res = match (opts::get().headless, get_selected_files(patterns, multiple_files)) {
                        (true, _) | (false, None) => sender.send(None),
                        (false, Some(files)) => sender.send(Some(files))
                    };
                    if let Err(e) = res {
                        let reason = format!("Failed to send SelectFiles response: {}", e);
                        self.event_queue.push(WindowEvent::SendError(None, reason));
                    };
                }
                EmbedderMsg::ShowIME(_kind) => {
                }
                EmbedderMsg::HideIME => {
                }
            }
        }
    }
}

fn get_selected_files(patterns: Vec<FilterPattern>, multiple_files: bool) -> Option<Vec<String>> {
    let picker_name = if multiple_files { "Pick files" } else { "Pick a file" };
    thread::Builder::new().name(picker_name.to_owned()).spawn(move || {
        let mut filters = vec![];
        for p in patterns {
            let s = "*.".to_string() + &p.0;
            filters.push(s)
        }
        let filter_ref = &(filters.iter().map(|s| s.as_str()).collect::<Vec<&str>>()[..]);
        let filter_opt = if filters.len() > 0 { Some((filter_ref, "")) } else { None };

        if multiple_files {
            tinyfiledialogs::open_file_dialog_multi(picker_name, "", filter_opt)
        } else {
            let file = tinyfiledialogs::open_file_dialog(picker_name, "", filter_opt);
            file.map(|x| vec![x])
        }
    }).unwrap().join().expect("Thread spawning failed")
}