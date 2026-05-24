"""
test_cases.py — benchmark question definitions

Each entry:
  id       : question ID used with --question flag (e.g. Q1)
  function : C function name being queried
  question : implementation-detail question that requires reading source code
  rubric   : list of {criterion: str, points: int} — used by judge.py for blind scoring
"""

TEST_CASES: list[dict] = [
    {
        "id": "Q1",
        "function": "archive_alloc",
        "question": (
            "What is the exact return type and parameter list of `archive_alloc`? "
            "List every furi record it opens and every view it registers with the view "
            "dispatcher, in the order they appear in the source."
        ),
        "rubric": [
            {"criterion": "Return type is exactly `ArchiveApp*`", "points": 1},
            {"criterion": "Parameter list is `void`", "points": 1},
            {"criterion": "Opens exactly two furi records in order: `RECORD_GUI` then `RECORD_LOADER`", "points": 1},
            {"criterion": "Names all three registered views: `ArchiveViewBrowser`, `ArchiveViewTextInput`, `ArchiveViewWidget`", "points": 1},
            {"criterion": "Views listed in correct source order: ArchiveViewBrowser → ArchiveViewTextInput → ArchiveViewWidget", "points": 1},
        ],
    },
    {
        "id": "Q2",
        "function": "archive_free",
        "question": (
            "What is the exact signature of `archive_free` (return type and parameters)? "
            "List each resource it releases in source order, naming the exact "
            "deallocation function used for each."
        ),
        "rubric": [
            {"criterion": "Signature is `void archive_free(ArchiveApp* archive)`", "points": 1},
            {"criterion": "Removes all three views from view_dispatcher before calling `view_dispatcher_free`", "points": 1},
            {"criterion": "Closes furi records in reverse-open order: `RECORD_LOADER` then `RECORD_GUI`", "points": 1},
            {"criterion": "`free(archive)` is the final deallocation call", "points": 1},
            {"criterion": "Mentions `furi_string_free(archive->fav_move_str)` and `text_input_free(archive->text_input)`", "points": 1},
        ],
    },
    {
        "id": "Q3",
        "function": "archive_custom_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`archive_custom_event_callback`? "
            "What single function does it call, and what arguments does it pass?"
        ),
        "rubric": [
            {"criterion": "Return type `bool`, parameters `void* context` and `uint32_t event`", "points": 1},
            {"criterion": "Calls `scene_manager_handle_custom_event`", "points": 1},
            {"criterion": "Passes `archive->scene_manager` and `event` as arguments", "points": 1},
        ],
    },
    {
        "id": "Q4",
        "function": "gpio_app_alloc",
        "question": (
            "What is the exact return type of `gpio_app_alloc`? "
            "How many views does it add to the view dispatcher? "
            "Name each view and the integer constant used as its view ID."
        ),
        "rubric": [
            {"criterion": "Return type is exactly `GpioApp*`", "points": 1},
            {"criterion": "Registers exactly 6 views", "points": 1},
            {"criterion": "All 6 view IDs correct: `GpioAppViewExitConfirm`, `GpioAppViewVarItemList`, `GpioAppViewGpioTest`, `GpioAppViewUsbUartCloseRpc`, `GpioAppViewUsbUart`, `GpioAppViewUsbUartCfg`", "points": 1},
            {"criterion": "Views listed in correct source order (ExitConfirm → VarItemList → GpioTest → UsbUartCloseRpc → UsbUart → UsbUartCfg)", "points": 1},
        ],
    },
    {
        "id": "Q5",
        "function": "gpio_app_custom_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`gpio_app_custom_event_callback`? "
            "What function does it call, and what value does it pass as the event?"
        ),
        "rubric": [
            {"criterion": "Declared `static bool`, parameters `void* context` and `uint32_t event`", "points": 1},
            {"criterion": "Calls `scene_manager_handle_custom_event`", "points": 1},
            {"criterion": "Passes `event` (the uint32_t parameter) as the event argument", "points": 1},
        ],
    },
    {
        "id": "Q6",
        "function": "gpio_app_back_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`gpio_app_back_event_callback`? "
            "What does it return, and does it call any other function?"
        ),
        "rubric": [
            {"criterion": "Declared `static bool`, parameter `void* context`", "points": 1},
            {"criterion": "Returns the result of `scene_manager_handle_back_event(app->scene_manager)`", "points": 1},
            {"criterion": "Calls exactly one function: `scene_manager_handle_back_event` (no other calls)", "points": 1},
        ],
    },
    {
        "id": "Q7",
        "function": "gpio_app_tick_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`gpio_app_tick_event_callback`? "
            "What function does it call, and what arguments does it pass?"
        ),
        "rubric": [
            {"criterion": "Declared `static void`, parameter `void* context`", "points": 1},
            {"criterion": "Calls `scene_manager_handle_tick_event`", "points": 1},
            {"criterion": "Passes `app->scene_manager` as the sole argument", "points": 1},
        ],
    },
    {
        "id": "Q8",
        "function": "nfc_app_alloc",
        "question": (
            "What is the exact return type of `nfc_app_alloc`? "
            "Name every furi record it opens and every storage/protocol "
            "initialisation call it makes, in source order."
        ),
        "rubric": [
            {"criterion": "Return type is exactly `NfcApp*`", "points": 1},
            {"criterion": "Opens exactly 4 furi records in order: `RECORD_GUI`, `RECORD_NOTIFICATION`, `RECORD_STORAGE`, `RECORD_DIALOGS`", "points": 2},
            {"criterion": "Protocol/init allocs listed in source order: nfc_alloc, nfc_detected_protocols_alloc, felica_auth_alloc, mf_ultralight_auth_alloc, slix_unlock_alloc, mf_classic_key_cache_alloc, nfc_supported_cards_alloc, nfc_device_alloc", "points": 2},
        ],
    },
    {
        "id": "Q9",
        "function": "nfc_app_rpc_command_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`nfc_app_rpc_command_callback`? "
            "What does it do when the event type is `RpcAppEventTypeSessionClose`, "
            "and what does it do for all other event types?"
        ),
        "rubric": [
            {"criterion": "Declared `static void`, parameters `const RpcAppSystemEvent* event` and `void* context`", "points": 1},
            {"criterion": "`RpcAppEventTypeSessionClose`: sends `NfcCustomEventRpcSessionClose`, then sets callback to NULL and sets `nfc->rpc_ctx` to NULL", "points": 2},
            {"criterion": "`RpcAppEventTypeAppExit`: sends `NfcCustomEventRpcExit` via `view_dispatcher_send_custom_event`", "points": 1},
            {"criterion": "`RpcAppEventTypeLoadFile`: copies the file path string, sends `NfcCustomEventRpcLoadFile`; any other type: calls `rpc_system_app_confirm(nfc->rpc_ctx, false)`", "points": 1},
        ],
    },
    {
        "id": "Q10",
        "function": "nfc_custom_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`nfc_custom_event_callback`? "
            "What single function does it delegate to, and what arguments does it pass?"
        ),
        "rubric": [
            {"criterion": "Return type `bool`, parameters `void* context` and `uint32_t event`", "points": 1},
            {"criterion": "Delegates to `scene_manager_handle_custom_event`", "points": 1},
            {"criterion": "Passes `nfc->scene_manager` and `event` as arguments", "points": 1},
        ],
    },
    {
        "id": "Q11",
        "function": "infrared_back_event_callback",
        "question": (
            "What are the exact parameters and return type of "
            "`infrared_back_event_callback`? "
            "What function does it call when the back event fires, "
            "and with what arguments?"
        ),
        "rubric": [
            {"criterion": "Declared `static bool`, parameter `void* context`", "points": 1},
            {"criterion": "Calls `scene_manager_handle_back_event`", "points": 1},
            {"criterion": "Passes `infrared->scene_manager` as the argument", "points": 1},
        ],
    },
    {
        "id": "Q12",
        "function": "infrared_make_app_folder",
        "question": (
            "What are the exact parameters and return type of `infrared_make_app_folder`? "
            "What is the exact folder path string it creates, "
            "and what storage API call does it use?"
        ),
        "rubric": [
            {"criterion": "Declared `static void`, parameter `InfraredApp* infrared`", "points": 1},
            {"criterion": "Folder path resolves to `/ext/infrared` (via `INFRARED_APP_FOLDER = EXT_PATH(\"infrared\")`)", "points": 1},
            {"criterion": "Storage API call is `storage_simply_mkdir`", "points": 1},
            {"criterion": "On failure calls `infrared_show_error_message` with message `\"Cannot create\\napp folder\"`", "points": 1},
        ],
    },
]
