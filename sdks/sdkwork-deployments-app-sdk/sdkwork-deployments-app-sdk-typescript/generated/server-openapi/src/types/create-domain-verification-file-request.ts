export interface CreateDomainVerificationFileRequest {
  /** 签发平台 */
  platform: 'WECHAT_OFFICIAL_ACCOUNT' | 'WECHAT_MINI_PROGRAM' | 'ALIPAY' | 'BYTEDANCE' | 'APPLE' | 'GOOGLE' | 'CUSTOM';
  /** 平台发放的文件名，单段 ASCII，以 .txt / .html / .xml 结尾 */
  fileName: string;
  /** 文件原文，逐字节保存与返回 */
  content: string;
}
