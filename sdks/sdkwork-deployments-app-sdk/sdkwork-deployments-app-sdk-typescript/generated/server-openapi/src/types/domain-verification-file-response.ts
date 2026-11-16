export interface DomainVerificationFileResponse {
  id: string;
  zoneId: string;
  /** 签发平台。只用于控制台分组与审计，边缘按文件名应答 */
  platform: 'WECHAT_OFFICIAL_ACCOUNT' | 'WECHAT_MINI_PROGRAM' | 'ALIPAY' | 'BYTEDANCE' | 'APPLE' | 'GOOGLE' | 'CUSTOM';
  /** 平台发放的文件名(如 MP_verify_xxx.txt / google123.html)，按精确名称与原大小写服务 */
  fileName: string;
  /** 文件原文，逐字节保存与返回 */
  content: string;
  /** 首次登记时间 */
  createdAt: string;
  /** 最近一次上传/替换的时间 */
  updatedAt: string;
}
