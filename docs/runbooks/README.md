# runbooks

See `DOCUMENTATION_SPEC.md` section 2.

See `DOCUMENTATION_SPEC.md` section 2.


<!-- scaffold-module-runbooks:index -->
## Docker 运维四件套（bin/ 标准，OPERATIONS_SPEC.md §7）

| Runbook | 内容 |
| --- | --- |
| [deploy.md](deploy.md) / [deploy.en.md](deploy.en.md) | 安装 / 升级 / 回滚 / 下线（bin/docker-deploy.sh + bin/docker-image.sh） |
| [troubleshooting.md](troubleshooting.md) / [troubleshooting.en.md](troubleshooting.en.md) | 症状 → doctor 检查 → 处置 |
| [backup-restore.md](backup-restore.md) / [backup-restore.en.md](backup-restore.en.md) | 备份 / 校验 / 恢复 / 演练（bin/backup.sh） |
| [log-reference.md](log-reference.md) / [log-reference.en.md](log-reference.en.md) | 健康日志特征与失败签名（bin/docker-deploy.sh logs） |

## Identity provisioning

| Runbook | 内容 |
| --- | --- |
| [identity-provisioning-reconciliation.md](identity-provisioning-reconciliation.md) | `deploy_app.identity_status` 只读报表、失败/挂起应用的重试路径与 active + NULL key 异常排查（迁移 0008） |

## WeChat channel publishing

| Runbook | 内容 |
| --- | --- |
| [wechat-channel-publishing.md](wechat-channel-publishing.md) | 第三方平台开通与授权、发布命令、直连模式边界、配置变量与故障处置 |
