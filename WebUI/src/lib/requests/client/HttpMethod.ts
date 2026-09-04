/*
 * Copyright (C) Contributors to the Suwayomi project
 *
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at https://mozilla.org/MPL/2.0/.
 */

/**
 * Deliberately in its own module with zero imports.
 *
 * This enum used to live in RestClient.ts, so anything needing it (AppStorage,
 * for one) took a runtime dependency on the whole request client - which closed
 * an import cycle back through BaseClient and left classes in their temporal
 * dead zone at module-evaluation time. A leaf module cannot participate in a
 * cycle.
 */
export enum HttpMethod {
    GET = 'GET',
    POST = 'POST',
    PUT = 'PUT',
    PATCH = 'PATCH',
    DELETE = 'DELETE',
}
