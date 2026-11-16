// src/utils/plugins_lib.rs
//! designplugin_lib（安装名 `plugin_lib`）的 FFI 封装。
//!
//! 该库按操作前缀导出符号（`get_plugins` / `post_plugins` / `patch_plugins` /
//! `update_plugins` / `delete_plugins` 等），没有统一前缀，因此这里直接按
//! 导出名调用，不再拼 `post_`。
//!
//! 约定：`int symbol(const char *json_in, char *json_out)`，输出缓冲区 64 KiB，
//! 响应为 `{code, data, msg}`，成功 `code = 200`。
//!
//! 1.1.0 的列表接口（`get_plugins` / `get_functions` / `get_skills`）支持
//! `cursor` 游标分页，响应带 `next_cursor` / `has_more`；分页参数可用
//! [`list_request`] 构造，响应用 [`next_cursor`] / [`has_more`] 读取。
//!
//! 只封装调用层；请求字段与业务语义见 designplugin_lib 的 `docs/ffi.md`
//! 以及本仓库的 `docs/plugins_lib.md`。

use anyhow::{anyhow, bail, Context, Result};
use libloading::Library;
use serde_json::{json, Value};
use std::ffi::{CStr, CString};
use std::fmt;
use std::os::raw::{c_char, c_int};
use std::sync::OnceLock;

/// plugin_lib 在 libary/ 下的文件名（由 plugin_lib/dytes.json 的 exports 决定）。
#[cfg(windows)]
const PLUGIN_LIB_PATH: &str = "./libary/plugin_lib.dll";
#[cfg(target_os = "linux")]
const PLUGIN_LIB_PATH: &str = "./libary/plugin_lib.so";
#[cfg(target_os = "macos")]
const PLUGIN_LIB_PATH: &str = "./libary/plugin_lib.dylib";

/// plugin_lib 对外 FFI 的输出缓冲：64 KiB。
const OUT_BUF_SIZE: usize = 65536;

/// 列表接口的 `page_size` 上限（上游限制 `1..=100`）。
pub const MAX_PAGE_SIZE: u32 = 100;

/// 参数错误：缺必填字段、枚举值非法、无字段可更新。
pub const CODE_BAD_REQUEST: i64 = 400103;
/// 插件 / 能力 / 技能不存在。
pub const CODE_NOT_FOUND: i64 = 400404;
/// 插件 UUID、能力 UUID、技能 ID 已存在。
pub const CODE_CONFLICT: i64 = 400409;
/// 数据库调用失败（依赖库加载失败、SQL 执行失败）。
pub const CODE_DB_FAILED: i64 = 500201;
/// 插件目录、`dytes.json`、库文件读取失败。
pub const CODE_FILE_FAILED: i64 = 500300;

/// 导出符号名；与 designplugin_lib 的 `src/ffi/` 保持一致。
pub mod symbols {
    pub const GET_PLUGINS: &str = "get_plugins";
    pub const POST_PLUGINS: &str = "post_plugins";
    pub const PATCH_PLUGINS: &str = "patch_plugins";
    pub const UPDATE_PLUGINS: &str = "update_plugins";
    pub const DELETE_PLUGINS: &str = "delete_plugins";

    pub const GET_FUNCTIONS: &str = "get_functions";
    pub const POST_FUNCTION: &str = "post_function";
    pub const UPDATE_FUNCTION: &str = "update_function";
    pub const PATCH_FUNCTION: &str = "patch_function";
    pub const DELETE_FUNCTION: &str = "delete_function";

    pub const GET_SKILLS: &str = "get_skills";
    pub const GET_SKILL: &str = "get_skill";
    pub const POST_SKILLS: &str = "post_skills";
    pub const UPDATE_SKILLS: &str = "update_skills";
    pub const PATCH_SKILLS: &str = "patch_skills";
    pub const DELETE_SKILLS: &str = "delete_skills";

    pub const GET_PROMPTS: &str = "get_prompts";
    pub const UPDATE_PROMPTS: &str = "update_prompts";

    pub const GET_TASKFLOW: &str = "get_taskflow";
    pub const UPDATE_TASKFLOW: &str = "update_taskflow";

    pub const INSTALL_PLUGIN: &str = "install_plugin";
    pub const UPDATE_PLUGIN: &str = "update_plugin";
}

type FfiFn = unsafe extern "C" fn(*const c_char, *mut c_char) -> c_int;

static PLUGIN_LIB: OnceLock<Library> = OnceLock::new();

/// plugin_lib 返回的业务错误，保留 `code` 便于调用方区分“未找到”“冲突”等。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLibError {
    pub symbol: String,
    pub code: i64,
    pub msg: String,
}

impl fmt::Display for PluginLibError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "plugin_lib {} 错误 (code={}): {}",
            self.symbol, self.code, self.msg
        )
    }
}

impl std::error::Error for PluginLibError {}

/// 判断 error 是否为 plugin_lib 的指定错误码。
pub fn is_error_code(error: &anyhow::Error, code: i64) -> bool {
    error
        .downcast_ref::<PluginLibError>()
        .is_some_and(|error| error.code == code)
}

fn ensure_library() -> Result<&'static Library> {
    if let Some(lib) = PLUGIN_LIB.get() {
        return Ok(lib);
    }

    // SAFETY: 只加载进程内的 plugin_lib，符号按导出的 C ABI 签名调用。
    let lib = unsafe { Library::new(PLUGIN_LIB_PATH) }
        .with_context(|| format!("加载 {} 失败", PLUGIN_LIB_PATH))?;
    if PLUGIN_LIB.set(lib).is_ok() {
        println!("[plugins_lib] 加载 {} 成功", PLUGIN_LIB_PATH);
    }
    PLUGIN_LIB.get().context("plugin_lib 库未初始化")
}

/// 检查 FFI 返回值：`0` 已写入，`-1` 输出指针为空，`-2` 缓冲区不足。
pub fn check_ret(symbol: &str, ret: c_int, buf_size: usize) -> Result<()> {
    match ret {
        0 => Ok(()),
        -1 => bail!("plugin_lib {} json_out 为空指针", symbol),
        -2 => bail!("plugin_lib {} 输出超过 {} KiB", symbol, buf_size / 1024),
        other => bail!("plugin_lib {} FFI 返回 {}", symbol, other),
    }
}

/// 解析 plugin_lib 的 JSON 响应并取出 `data`。
///
/// 纯函数，便于单测。业务错误（`code != 200`）转为 [`PluginLibError`]。
pub fn parse_response(symbol: &str, out: &str) -> Result<Value> {
    if out.trim().is_empty() {
        bail!("plugin_lib {} 返回空", symbol);
    }

    let value: Value = serde_json::from_str(out)
        .with_context(|| format!("plugin_lib {} 响应解析失败: {}", symbol, out))?;

    let code = value.get("code").and_then(Value::as_i64).unwrap_or(-1);
    if code != 200 {
        let msg = value
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        return Err(anyhow!(PluginLibError {
            symbol: symbol.to_string(),
            code,
            msg,
        }));
    }

    Ok(value.get("data").cloned().unwrap_or(Value::Null))
}

/// 调用任意 plugin_lib 导出符号，返回响应中的 `data`。
pub fn call_ffi(symbol: &str, request: &Value) -> Result<Value> {
    let body = request.to_string();
    println!(
        "[plugins_lib] call_ffi: symbol={}, request={:?}",
        symbol,
        preview(&body, 300)
    );

    let lib = ensure_library()?;
    // SAFETY: plugin_lib 的导出函数都是 `extern "C" fn(*const c_char, *mut c_char) -> c_int`。
    let function: libloading::Symbol<'_, FfiFn> = unsafe {
        lib.get(symbol.as_bytes())
            .with_context(|| format!("plugin_lib 缺少导出符号 {}", symbol))?
    };

    let req = CString::new(body).context("请求 JSON 含 \\0")?;
    let mut out_buf = vec![0u8; OUT_BUF_SIZE];
    // SAFETY: 请求与输出缓冲区生命周期覆盖整个调用。
    let ret = unsafe { function(req.as_ptr(), out_buf.as_mut_ptr() as *mut c_char) };
    check_ret(symbol, ret, OUT_BUF_SIZE)?;

    let out = unsafe { CStr::from_ptr(out_buf.as_ptr() as *const c_char) }
        .to_str()
        .context("plugin_lib 响应非 UTF-8")?;
    parse_response(symbol, out)
}

/// 构造列表接口的分页 / 游标参数；未提供的字段不会写入请求。
///
/// `get_plugins` / `get_functions` / `get_skills` 支持 `cursor`：把上一页
/// 响应中的 `next_cursor` 原样传入即可；传 `cursor` 时上游忽略 `page`。
pub fn list_request(page: Option<u32>, page_size: Option<u32>, cursor: Option<&str>) -> Value {
    let mut request = serde_json::Map::new();
    if let Some(page) = page {
        request.insert("page".to_string(), json!(page));
    }
    if let Some(page_size) = page_size {
        request.insert("page_size".to_string(), json!(page_size));
    }
    if let Some(cursor) = cursor {
        request.insert("cursor".to_string(), json!(cursor));
    }
    Value::Object(request)
}

/// 读取列表响应中的 `next_cursor`；没有下一页时为 `None`。
pub fn next_cursor(data: &Value) -> Option<&str> {
    data.get("next_cursor").and_then(Value::as_str)
}

/// 列表响应是否还有下一页；字段缺失时按 `false` 处理。
pub fn has_more(data: &Value) -> bool {
    data.get("has_more")
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

/// 按字符截断字符串，避免日志刷屏。
pub fn preview(value: &str, max: usize) -> String {
    let mut out = String::new();
    for (index, ch) in value.chars().enumerate() {
        if index >= max {
            out.push_str("...");
            break;
        }
        out.push(ch);
    }
    out
}

/// 查询插件列表；请求中出现 `uuid` / `name` 时返回详情。
pub fn get_plugins(request: &Value) -> Result<Value> {
    call_ffi(symbols::GET_PLUGINS, request)
}

/// 新建插件。
pub fn post_plugins(request: &Value) -> Result<Value> {
    call_ffi(symbols::POST_PLUGINS, request)
}

/// 局部更新插件（`enabled` / `status` / `cover` / `mask`）。
pub fn patch_plugins(request: &Value) -> Result<Value> {
    call_ffi(symbols::PATCH_PLUGINS, request)
}

/// 全量更新插件主表字段。
pub fn update_plugins(request: &Value) -> Result<Value> {
    call_ffi(symbols::UPDATE_PLUGINS, request)
}

/// 删除插件及其能力记录（物理删除）。
pub fn delete_plugins(request: &Value) -> Result<Value> {
    call_ffi(symbols::DELETE_PLUGINS, request)
}

/// 查询插件能力列表。
pub fn get_functions(request: &Value) -> Result<Value> {
    call_ffi(symbols::GET_FUNCTIONS, request)
}

/// 新建插件能力。
pub fn post_function(request: &Value) -> Result<Value> {
    call_ffi(symbols::POST_FUNCTION, request)
}

/// 全量更新插件能力。
pub fn update_function(request: &Value) -> Result<Value> {
    call_ffi(symbols::UPDATE_FUNCTION, request)
}

/// 局部更新插件能力。
pub fn patch_function(request: &Value) -> Result<Value> {
    call_ffi(symbols::PATCH_FUNCTION, request)
}

/// 删除插件能力。
pub fn delete_function(request: &Value) -> Result<Value> {
    call_ffi(symbols::DELETE_FUNCTION, request)
}

/// 查询技能摘要列表（不含 `content`）。
pub fn get_skills(request: &Value) -> Result<Value> {
    call_ffi(symbols::GET_SKILLS, request)
}

/// 读取单条技能完整文档（含 `content`）。
pub fn get_skill(request: &Value) -> Result<Value> {
    call_ffi(symbols::GET_SKILL, request)
}

/// 新建技能。
pub fn post_skills(request: &Value) -> Result<Value> {
    call_ffi(symbols::POST_SKILLS, request)
}

/// 更新技能（传入字段增量更新，`version + 1`）。
pub fn update_skills(request: &Value) -> Result<Value> {
    call_ffi(symbols::UPDATE_SKILLS, request)
}

/// 局部更新技能（与 [`update_skills`] 同一实现）。
pub fn patch_skills(request: &Value) -> Result<Value> {
    call_ffi(symbols::PATCH_SKILLS, request)
}

/// 删除技能。
pub fn delete_skills(request: &Value) -> Result<Value> {
    call_ffi(symbols::DELETE_SKILLS, request)
}

/// 读取某个插件的提示词。
pub fn get_prompts(request: &Value) -> Result<Value> {
    call_ffi(symbols::GET_PROMPTS, request)
}

/// 整体覆盖某个插件的提示词。
pub fn update_prompts(request: &Value) -> Result<Value> {
    call_ffi(symbols::UPDATE_PROMPTS, request)
}

/// 读取全局任务流。
pub fn get_taskflow(request: &Value) -> Result<Value> {
    call_ffi(symbols::GET_TASKFLOW, request)
}

/// 整体覆盖全局任务流。
pub fn update_taskflow(request: &Value) -> Result<Value> {
    call_ffi(symbols::UPDATE_TASKFLOW, request)
}

/// 安装插件包（`dir` 为包目录）。
pub fn install_plugin(request: &Value) -> Result<Value> {
    call_ffi(symbols::INSTALL_PLUGIN, request)
}

/// 更新已安装插件包。
pub fn update_plugin(request: &Value) -> Result<Value> {
    call_ffi(symbols::UPDATE_PLUGIN, request)
}
