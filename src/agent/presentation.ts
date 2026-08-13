export interface BackendState {
  available: boolean
  authenticated: boolean | null
  errorCode: string | null
}

export interface BackendPresentation {
  ready: boolean
  label: string
  title: string | null
  instruction: string | null
}

export function backendPresentation(status: BackendState | null): BackendPresentation {
  if (!status) {
    return { ready: false, label: '正在检查 Claude Code', title: null, instruction: null }
  }
  if (!status.available || status.errorCode === 'agent.backend_unavailable') {
    return {
      ready: false,
      label: '未安装 Claude Code',
      title: '先安装解析器',
      instruction: '安装 Claude Code CLI 后重启日簿；账本功能仍可离线打开。',
    }
  }
  if (status.authenticated === false || status.errorCode === 'agent.not_authenticated') {
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
  return { ready: true, label: '考古员已就绪', title: null, instruction: null }
}
