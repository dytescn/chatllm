// src/ffi/chat.rs
//
// chatllm 对外只有三个 FFI 接口，调用约定见 `docs/ffi.md`：
// `int (*)(const char *json_in, char *json_out);`
// 本层只做输入解析与错误码映射，业务逻辑在 `utils::dialogue`。
use crate::ffi::types::{ChatFlow, Dialogue, DialogueListReq, DialogueQuery};
use crate::utils::cjson::{err_msg, read_body, write_error, write_success};
use crate::utils::dialogue;
use serde_json::json;
use std::ffi::{c_char, c_int};

/// 对话列表。
///
/// FFI-Symbol：`patch_dialogue`
/// 入参：`DialogueListReq`（`keyword` / `page` / `page_size`，均可空，允许空 body）
/// 出参：`{ list, page, page_size, total }`
#[unsafe(no_mangle)]
pub extern "C" fn patch_dialogue(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = if json_in.is_null() {
        ""
    } else {
        match read_body(json_in) {
            Ok(body) => body,
            Err(code) => return write_error(json_out, code, err_msg(code)),
        }
    };
    let req: DialogueListReq = if body.trim().is_empty() {
        DialogueListReq {
            keyword: None,
            page: None,
            page_size: None,
        }
    } else {
        match serde_json::from_str(body) {
            Ok(req) => req,
            Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
        }
    };

    match dialogue::query_dialogue(req.keyword.as_deref(), req.page, req.page_size) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, 400302, &format!("list dialogue failed: {:#}", e)),
    }
}

/// 读取一条对话及其消息。
///
/// FFI-Symbol：`get_dialogue`
/// 入参：`DialogueQuery`（`uuid` 必填，`before_id` / `after_id` / `limit` 可选）
/// 出参：`{ dialogue: {...}, messages: { list, limit, first_id, last_id, has_more_before, has_more_after } }`
#[unsafe(no_mangle)]
pub extern "C" fn get_dialogue(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: DialogueQuery = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match dialogue::dialogue_detail(&req.uuid, req.before_id, req.after_id, req.limit) {
        Ok(data) => write_success(json_out, "success", data),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}

/// 更新对话信息。
///
/// FFI-Symbol：`update_dialogue`
/// 入参：`Dialogue`（`uuid` 定位对话，`title` / `show_status` 为待更新项）
/// 出参：`{ id }`
#[unsafe(no_mangle)]
pub extern "C" fn update_dialogue(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: Dialogue = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    match dialogue::update_chat(
        req.uuid.as_deref(),
        req.title.as_deref(),
        req.show_status,
        req.is_pinned,
        req.is_archived,
    ) {
        Ok(id) => write_success(json_out, "success", json!({ "id": id })),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}

/// 创建一条对话。
///
/// FFI-Symbol：`chat`
/// 入参：`ChatFlow`（`prompt` 必填；`uuid` 非空时续聊已有对话）
/// 出参：`{ uuid, created, prompt, max_tokens, route }`
#[unsafe(no_mangle)]
pub extern "C" fn post_message(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    let body = match read_body(json_in) {
        Ok(body) => body,
        Err(code) => return write_error(json_out, code, err_msg(code)),
    };
    let req: ChatFlow = match serde_json::from_str(body) {
        Ok(req) => req,
        Err(e) => return write_error(json_out, 400102, &format!("invalid json: {}", e)),
    };

    // check 阶段：配置缺失时直接返回，不创建对话、不写消息。
    if let Some(message) = crate::utils::check::missing_supplier_reply() {
        println!("[chat] check 未通过，直接返回设置提示");
        return write_success(
            json_out,
            "success",
            json!({
                "uuid": null,
                "created": false,
                "prompt": req.prompt,
                "max_tokens": req.max_tokens,
                "route": crate::utils::check::SETTING_ROUTE,
                "content": message,
                "content_html": "",
            }),
        );
    }

    // think 阶段：命中 chat_think 后取出资源包 uuid/name，用于定位对话与 skills。
    let (think_plugin_uuid, system_prompt) = {
        match crate::utils::think::find(&req.prompt, crate::utils::think::TOP_K) {
            Ok(Some(hit)) => {
                let system = hit
                    .plugin_name
                    .as_deref()
                    .and_then(crate::utils::think::skills_for);
                (hit.plugin_uuid, system)
            }
            Ok(None) => (None, None),
            Err(error) => {
                println!("[chat] think 检索失败: {:#}", error);
                (None, None)
            }
        }
    };
    // 只有调用方显式传入的 plugin_uuid 才能作为对话 id；think 命中的 uuid
    // 只用于标识插件 / 读取 skills，不能复用为对话 id，否则命中同一插件的
    // 多个对话会互相覆盖。
    let dialogue_plugin_uuid = req.plugin_uuid.as_deref();
    let response_plugin_uuid = req.plugin_uuid.as_deref().or(think_plugin_uuid.as_deref());

    match dialogue::open_chat(
        &req.prompt,
        req.uuid.as_deref(),
        dialogue_plugin_uuid,
        req.title.as_deref(),
        req.show_status,
        system_prompt.as_deref(),
    ) {
        Ok((uuid, created)) => write_success(
            json_out,
            "success",
            json!({
                "uuid": uuid,
                "created": created,
                "prompt": req.prompt,
                "max_tokens": req.max_tokens,
                "route": null,
                "plugin_uuid": response_plugin_uuid,
            }),
        ),
        Err(e) => write_error(json_out, e.code(), &e.message()),
    }
}

/// `chat` 的导出入口。
///
/// cyctron 按 `FFI-Symbol: chat` 拼成 `post_chat`，这里作为
/// [`post_message`] 的别名，保持宿主 / 文档调用一致。
#[unsafe(no_mangle)]
pub extern "C" fn post_chat(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    post_message(json_in, json_out)
}

/// `dialogue_list` 的导出入口。
#[unsafe(no_mangle)]
pub extern "C" fn post_dialogue_list(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    patch_dialogue(json_in, json_out)
}

/// `dialogue_search` 的导出入口。
#[unsafe(no_mangle)]
pub extern "C" fn post_dialogue_search(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    patch_dialogue(json_in, json_out)
}

/// `dialogue_update` 的导出入口。
#[unsafe(no_mangle)]
pub extern "C" fn post_dialogue_update(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    update_dialogue(json_in, json_out)
}

/// `chat_detail` 的导出入口。
#[unsafe(no_mangle)]
pub extern "C" fn post_chat_detail(json_in: *const c_char, json_out: *mut c_char) -> c_int {
    get_dialogue(json_in, json_out)
}
