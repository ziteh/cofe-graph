"""
test_cases.py — benchmark question definitions

Each entry:
  id       : question ID used with --question flag (e.g. Q1)
  function : C function name being queried
  question : implementation-detail question that requires reading source code
  difficulty_weight : float — score multiplier (1.0=easy, 1.5=medium, 2.0=hard)
  rubric   : list of {criterion: str, points: int} — used by judge.py for blind scoring
"""

TEST_CASES: list[dict] = [
    {
        "id": "Q1",
        "difficulty_weight": 1.5,
        "function": "archive_alloc",
        "question": (
            "What is the exact return type and parameter list of `archive_alloc`? "
            "List every furi record it opens and every view it registers with the view "
            "dispatcher, in the order they appear in the source."
        ),
        "rubric": [
            {"criterion": "Return type is exactly `ArchiveApp*`", "points": 1},
            {"criterion": "Parameter list is `void`", "points": 1},
            {
                "criterion": "Opens exactly two furi records in order: `RECORD_GUI` then `RECORD_LOADER`",
                "points": 1,
            },
            {
                "criterion": "Names all three registered views: `ArchiveViewBrowser`, `ArchiveViewTextInput`, `ArchiveViewWidget`",
                "points": 1,
            },
            {
                "criterion": "Views listed in correct source order: ArchiveViewBrowser → ArchiveViewTextInput → ArchiveViewWidget",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q2",
        "difficulty_weight": 1.5,
        "function": "archive_free",
        "question": (
            "What is the exact signature of `archive_free` (return type and parameters)? "
            "List each resource it releases in source order, naming the exact "
            "deallocation function used for each."
        ),
        "rubric": [
            {
                "criterion": "Signature is `void archive_free(ArchiveApp* archive)`",
                "points": 1,
            },
            {
                "criterion": "Removes all three views from view_dispatcher before calling `view_dispatcher_free`",
                "points": 1,
            },
            {
                "criterion": "Closes furi records in reverse-open order: `RECORD_LOADER` then `RECORD_GUI`",
                "points": 1,
            },
            {
                "criterion": "`free(archive)` is the final deallocation call",
                "points": 1,
            },
            {
                "criterion": "Mentions `furi_string_free(archive->fav_move_str)` and `text_input_free(archive->text_input)`",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q3",
        "difficulty_weight": 1.0,
        "function": "archive_custom_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`archive_custom_event_callback`? "
            "What single function does it call, and what arguments does it pass?"
        ),
        "rubric": [
            {
                "criterion": "Return type `bool`, parameters `void* context` and `uint32_t event`",
                "points": 1,
            },
            {"criterion": "Calls `scene_manager_handle_custom_event`", "points": 1},
            {
                "criterion": "Passes `archive->scene_manager` and `event` as arguments",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q4",
        "difficulty_weight": 1.5,
        "function": "gpio_app_alloc",
        "question": (
            "What is the exact return type of `gpio_app_alloc`? "
            "How many views does it add to the view dispatcher? "
            "Name each view and the integer constant used as its view ID."
        ),
        "rubric": [
            {"criterion": "Return type is exactly `GpioApp*`", "points": 1},
            {"criterion": "Registers exactly 6 views", "points": 1},
            {
                "criterion": "All 6 view IDs correct: `GpioAppViewExitConfirm`, `GpioAppViewVarItemList`, `GpioAppViewGpioTest`, `GpioAppViewUsbUartCloseRpc`, `GpioAppViewUsbUart`, `GpioAppViewUsbUartCfg`",
                "points": 1,
            },
            {
                "criterion": "Views listed in correct source order (ExitConfirm → VarItemList → GpioTest → UsbUartCloseRpc → UsbUart → UsbUartCfg)",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q6",
        "difficulty_weight": 1.0,
        "function": "gpio_app_back_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`gpio_app_back_event_callback`? "
            "What does it return, and does it call any other function?"
        ),
        "rubric": [
            {
                "criterion": "Declared `static bool`, parameter `void* context`",
                "points": 1,
            },
            {
                "criterion": "Returns the result of `scene_manager_handle_back_event(app->scene_manager)`",
                "points": 1,
            },
            {
                "criterion": "Calls exactly one function: `scene_manager_handle_back_event` (no other calls)",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q8",
        "difficulty_weight": 2.0,
        "function": "nfc_app_alloc",
        "question": (
            "What is the exact return type of `nfc_app_alloc`? "
            "Name every furi record it opens and every storage/protocol "
            "initialisation call it makes, in source order."
        ),
        "rubric": [
            {"criterion": "Return type is exactly `NfcApp*`", "points": 1},
            {
                "criterion": "Opens exactly 4 furi records in order: `RECORD_GUI`, `RECORD_NOTIFICATION`, `RECORD_STORAGE`, `RECORD_DIALOGS`",
                "points": 2,
            },
            {
                "criterion": "Protocol/init allocs listed in source order: nfc_alloc, nfc_detected_protocols_alloc, felica_auth_alloc, mf_ultralight_auth_alloc, slix_unlock_alloc, mf_classic_key_cache_alloc, nfc_supported_cards_alloc, nfc_device_alloc",
                "points": 2,
            },
        ],
    },
    {
        "id": "Q9",
        "difficulty_weight": 2.0,
        "function": "nfc_app_rpc_command_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`nfc_app_rpc_command_callback`? "
            "What does it do when the event type is `RpcAppEventTypeSessionClose`, "
            "and what does it do for all other event types?"
        ),
        "rubric": [
            {
                "criterion": "Declared `static void`, parameters `const RpcAppSystemEvent* event` and `void* context`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeSessionClose`: sends `NfcCustomEventRpcSessionClose`, then sets callback to NULL and sets `nfc->rpc_ctx` to NULL",
                "points": 2,
            },
            {
                "criterion": "`RpcAppEventTypeAppExit`: sends `NfcCustomEventRpcExit` via `view_dispatcher_send_custom_event`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeLoadFile`: copies the file path string, sends `NfcCustomEventRpcLoadFile`; any other type: calls `rpc_system_app_confirm(nfc->rpc_ctx, false)`",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q12",
        "difficulty_weight": 1.0,
        "function": "infrared_make_app_folder",
        "question": (
            "What are the exact parameters and return type of `infrared_make_app_folder`? "
            "What is the exact folder path string it creates, "
            "and what storage API call does it use?"
        ),
        "rubric": [
            {
                "criterion": "Declared `static void`, parameter `InfraredApp* infrared`",
                "points": 1,
            },
            {
                "criterion": 'Folder path resolves to `/ext/infrared` (via `INFRARED_APP_FOLDER = EXT_PATH("infrared")`)',
                "points": 1,
            },
            {"criterion": "Storage API call is `storage_simply_mkdir`", "points": 1},
            {
                "criterion": 'On failure calls `infrared_show_error_message` with message `"Cannot create\\napp folder"`',
                "points": 1,
            },
        ],
    },
    {
        "id": "Q13",
        "difficulty_weight": 1.0,
        "function": "infrared_tx_stop",
        "question": (
            "What is the exact signature of `infrared_tx_stop` (return type and parameters)? "
            "What guard condition causes it to return early without doing anything? "
            "List every action it performs in source order when it does not return early."
        ),
        "rubric": [
            {
                "criterion": "Signature is `void infrared_tx_stop(InfraredApp* infrared)`",
                "points": 1,
            },
            {
                "criterion": "Returns early when `infrared->app_state.is_transmitting` is false",
                "points": 1,
            },
            {
                "criterion": "Calls `infrared_worker_tx_stop(infrared->worker)` first",
                "points": 1,
            },
            {
                "criterion": "Calls `infrared_worker_tx_set_get_signal_callback(infrared->worker, NULL, NULL)` next",
                "points": 1,
            },
            {
                "criterion": "Calls `infrared_play_notification_message` with `InfraredNotificationMessageBlinkStop`, then sets `is_transmitting = false` and records `last_transmit_time = furi_get_tick()`",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q14",
        "difficulty_weight": 1.5,
        "function": "nfc_delete",
        "question": (
            "What is the exact signature of `nfc_delete` (return type and parameters)? "
            "Describe every step the function performs in source order, including "
            "how it handles a shadow file when one exists and how it adjusts `file_path` "
            "before the final storage call."
        ),
        "rubric": [
            {
                "criterion": "Signature is `bool nfc_delete(NfcApp* instance)`",
                "points": 1,
            },
            {
                "criterion": "Checks for a shadow file with `nfc_has_shadow_file` and calls `nfc_delete_shadow_file` when one exists",
                "points": 1,
            },
            {
                "criterion": "If `file_path` ends with `NFC_APP_SHADOW_EXTENSION`, replaces the last 4 characters with `NFC_APP_EXTENSION` via `furi_string_replace_at`",
                "points": 2,
            },
            {
                "criterion": "Final call is `storage_simply_remove(instance->storage, ...)` and its return value is returned",
                "points": 1,
            },
        ],
    },
    {
        "id": "Q15",
        "difficulty_weight": 2.0,
        "function": "infrared_rpc_command_callback",
        "question": (
            "What are the exact parameters and return type of `infrared_rpc_command_callback`? "
            "For each branch of the event-type dispatch, state the event type, "
            "any data-type sub-condition, and the exact custom event constant sent "
            "(or other action taken). Include the default/else branch."
        ),
        "rubric": [
            {
                "criterion": "Declared `static void`, parameters `const RpcAppSystemEvent* event` and `void* context`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeSessionClose`: sends `InfraredCustomEventTypeRpcSessionClose`, sets callback to NULL via `rpc_system_app_set_callback`, then sets `infrared->rpc_ctx = NULL`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeAppExit`: sends `InfraredCustomEventTypeRpcExit`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeLoadFile`: copies string to `infrared->file_path`, sends `InfraredCustomEventTypeRpcLoadFile`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeButtonPress` with `RpcAppSystemEventDataTypeString`: sets `infrared->button_name`, sends `InfraredCustomEventTypeRpcButtonPressName`; with `RpcAppSystemEventDataTypeInt32`: sets `infrared->app_state.current_button_index`, sends `InfraredCustomEventTypeRpcButtonPressIndex`",
                "points": 1,
            },
            {
                "criterion": "`RpcAppEventTypeButtonPressRelease` with string: sends `InfraredCustomEventTypeRpcButtonPressReleaseName`; with int32: sends `InfraredCustomEventTypeRpcButtonPressReleaseIndex`; `RpcAppEventTypeButtonRelease`: sends `InfraredCustomEventTypeRpcButtonRelease`; else: calls `rpc_system_app_confirm(infrared->rpc_ctx, false)`",
                "points": 1,
            },
        ],
    },
]
