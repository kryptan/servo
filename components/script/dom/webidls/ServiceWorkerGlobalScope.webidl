/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

// https://w3c.github.io/ServiceWorker/#service-worker-global-scope

[Global=(Worker,ServiceWorker), Exposed=ServiceWorker,
 Pref="dom.serviceworker.enabled"]
interface ServiceWorkerGlobalScope : WorkerGlobalScope {
};
