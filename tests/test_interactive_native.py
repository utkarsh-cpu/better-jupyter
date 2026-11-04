"""The same interactivity fixtures qualify both selected runtime owners."""

import pytest
from test_interactive_worker import (
    test_async_future_flags_and_cleanup as check_async,
)
from test_interactive_worker import (
    test_busy_reads_do_not_block_interrupt as check_busy,
)
from test_interactive_worker import (
    test_execution_conveniences_and_original_source as check_execution,
)
from test_interactive_worker import (
    test_history_collision_alias_and_checkpoint as check_history,
)
from test_interactive_worker import (
    test_live_reads_unicode_staleness_and_no_history as check_reads,
)
from test_interactive_worker import (
    test_restore_does_not_expose_reconstructing_namespace as check_restore,
)
from test_interactive_worker import (
    test_timing_errors_capture_and_exact_writefile as check_capture,
)
from test_interrupt_delivery import check_interrupt_delivery, delayed_execution_worker

pytest_plugins = ["test_native_supervisor"]


@pytest.mark.parametrize(
    "check",
    [
        check_async,
        check_busy,
        check_execution,
        check_history,
        check_reads,
        check_capture,
        check_restore,
    ],
)
def test_native_interactivity(native_runtime, check):
    check(native_runtime[0])


@pytest.mark.parametrize(
    "native_runtime", [{"env": {"KL_EXPERIMENTAL_INTERACTIVITY": "1"}}], indirect=True
)
def test_native_debug_control(native_runtime, monkeypatch):
    from test_worker_debug import test_debug_pause_step_claim_and_interrupt

    test_debug_pause_step_claim_and_interrupt(native_runtime[0], monkeypatch)


@pytest.mark.parametrize(
    "native_runtime", [{"env": {"KL_EXPERIMENTAL_INTERACTIVITY": "1"}}], indirect=True
)
def test_native_debug_frames_and_breakpoints(native_runtime, monkeypatch):
    from test_worker_debug import test_debug_post_mortem_and_prior_cell_breakpoint

    test_debug_post_mortem_and_prior_cell_breakpoint(native_runtime[0], monkeypatch)


@pytest.mark.parametrize(
    "native_runtime", [{"env": {"KL_EXPERIMENTAL_INTERACTIVITY": "1"}}], indirect=True
)
def test_native_widget_admission(native_runtime, monkeypatch):
    from test_widget_protocol import test_widget_callbacks_are_durable_and_retry_deduplicated

    test_widget_callbacks_are_durable_and_retry_deduplicated(native_runtime[0], monkeypatch)


def test_native_jupyter_attachment(native_runtime, tmp_path):
    from test_jupyter_adapter import test_real_jupyter_client_shared_namespace_completion_and_detach

    test_real_jupyter_client_shared_namespace_completion_and_detach(native_runtime[0], tmp_path)


def test_native_cross_cell_display_lifetime(native_runtime):
    from test_display_updates import (
        test_runtime_updates_are_shared_but_do_not_cross_worker_replacement,
    )

    test_runtime_updates_are_shared_but_do_not_cross_worker_replacement(native_runtime[0])


@pytest.mark.parametrize("native_runtime", [{"worker": delayed_execution_worker}], indirect=True)
@pytest.mark.parametrize("transport", ["legacy", "desktop"])
def test_native_interrupt_before_worker_running_ack_is_delivered_once(native_runtime, transport):
    check_interrupt_delivery(native_runtime[0], transport)
