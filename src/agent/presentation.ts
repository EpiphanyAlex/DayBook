export interface BackendState {
  available: boolean
  availabilityReason: string | null
  authenticated: boolean | null
  ready: boolean
  errorCode: string | null
}

export interface BackendPresentation {
  ready: boolean
  label: string
  title: string | null
  instruction: string | null
}

/**
 * 安装资格失败的三种原因给**三句不同的话**（01 §3.5）。
 * 「没找到」该去装，「不可执行」该去 `chmod`，「版本读不出来」该去修那个安装本身——
 * 都说成「未安装 Claude Code」的话，用户照着指引重装一遍也修不好。
 */
const AVAILABILITY_GUIDANCE: Record<string, { label: string; title: string; instruction: string }> =
  {
    not_found: {
      label: '未安装 Claude Code',
      title: '先安装解析器',
      instruction: '安装 Claude Code CLI 后重启日簿；账本功能仍可离线打开。',
    },
    not_executable: {
      label: 'Claude Code 无法执行',
      title: '修复解析器的执行权限',
      instruction:
        '找到了 claude，但它没有执行权限（或指向的不是可执行文件）。在终端运行 chmod +x 后重启日簿。',
    },
    version_unreadable: {
      label: 'Claude Code 版本读取失败',
      title: '修复这个安装',
      instruction:
        '找到了可执行的 claude，但 claude --version 没能正常返回版本号。先在终端确认它能跑，再重启日簿。',
    },
  }

const CHECKING: BackendPresentation = {
  ready: false,
  label: '正在检查 Claude Code',
  title: null,
  instruction: null,
}

/**
 * **`ready` 只认 `status.ready`**（01 §3.5）。
 *
 * 这里此前的兜底分支是 `return { ready: true, … }`——只要「available 且没有错误码」就宣布
 * 已就绪，而 readiness probe 要真跑一次 CLI 会话，那几秒里用户看到的是绿灯、点下去却失败。
 * 最近一次探测结论由 Rust 运行时持有，前端只负责把它翻译成话。
 */
export function backendPresentation(status: BackendState | null): BackendPresentation {
  if (!status) return CHECKING
  if (!status.available) {
    const guidance =
      AVAILABILITY_GUIDANCE[status.availabilityReason ?? 'not_found'] ??
      AVAILABILITY_GUIDANCE.not_found
    return { ready: false, ...guidance }
  }
  if (status.errorCode === 'agent.not_authenticated' || status.authenticated === false) {
    return {
      ready: false,
      label: 'Claude Code 尚未登录',
      title: '完成一次终端登录',
      instruction: '在终端运行 claude，按提示登录后回到日簿重试。',
    }
  }
  if (status.errorCode === 'agent.tool_surface_unsealed') {
    return {
      ready: false,
      label: 'Claude Code 安全检查未通过',
      title: '解析已被安全暂停',
      instruction: '当前 CLI 暴露了额外工具、插件或 hook；恢复密封配置后再解析。',
    }
  }
  if (status.errorCode) {
    return {
      ready: false,
      label: 'Claude Code 暂不可用',
      title: '解析器需要处理',
      instruction: `错误码：${status.errorCode}`,
    }
  }
  // 已发现合格 CLI、没有错误码，但探测还没跑完——这是一个**非错误**的中间态。
  if (!status.ready) return CHECKING
  return { ready: true, label: '考古员已就绪', title: null, instruction: null }
}
