/**
 * Copyright 2025 Schelling Point Labs Inc
 * SPDX-License-Identifier: AGPL-3.0-only
 */

// @refresh reload
import { mount, StartClient } from '@solidjs/start/client';

export default mount(() => <StartClient />, document.getElementById('app')!);
