/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use ipc_channel::ipc;

/// RAII fetch canceller object. By default initialized to not having a canceller
/// in it, however you can ask it for a cancellation receiver to send to Fetch
/// in which case it will store the sender. You can manually cancel it
/// or let it cancel on Drop in that case.
#[derive(Default, JSTraceable, MallocSizeOf)]
pub struct FetchCanceller {
    #[ignore_malloc_size_of = "channels are hard"]
    cancel_chan: Option<ipc::IpcSender<()>>
}

impl FetchCanceller {
    /// Create an empty FetchCanceller
    pub fn new() -> Self {
        Default::default()
    }

    /// Obtain an IpcReceiver to send over to Fetch, and initialize
    /// the internal sender
    pub fn initialize(&mut self) -> ipc::IpcReceiver<()> {
        // cancel previous fetch
        self.cancel();
        let (rx, tx) = ipc::channel().unwrap();
        self.cancel_chan = Some(rx);
        tx
    }

    /// Cancel a fetch if it is ongoing
    pub fn cancel(&mut self) {
        if let Some(chan) = self.cancel_chan.take() {
            // stop trying to make fetch happen
            // it's not going to happen

            // The receiver will be destroyed if the request has already completed;
            // so we throw away the error. Cancellation is a courtesy call,
            // we don't actually care if the other side heard.
            let _ = chan.send(());
        }
    }

    /// Use this if you don't want it to send a cancellation request
    /// on drop (e.g. if the fetch completes)
    pub fn ignore(&mut self) {
        let _ = self.cancel_chan.take();
    }
}

impl Drop for FetchCanceller {
    fn drop(&mut self) {
        self.cancel()
    }
}
