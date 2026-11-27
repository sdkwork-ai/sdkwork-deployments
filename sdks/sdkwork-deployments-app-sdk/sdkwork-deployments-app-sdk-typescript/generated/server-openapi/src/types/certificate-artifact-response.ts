/** 当前生效证书版本的公开材料 ZIP。私钥不在响应模型中，也不能写入归档。 */
export interface CertificateArtifactResponse {
  /** 下载时建议使用的 ZIP 文件名。 */
  fileName: string;
  mediaType: 'application/zip';
  /** ZIP 字节数。 */
  sizeBytes: string;
  /** ZIP 内容的 SHA-256 小写十六进制摘要。 */
  sha256: string;
  /** 有界 ZIP 字节的标准 base64 编码；绝不包含 privkey.pem。 */
  contentBase64: string;
}
