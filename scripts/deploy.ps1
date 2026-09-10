#Requires -Version 5.1
<#
.SYNOPSIS
  Deploy SDKWork module environments via Docker Compose (external dependency mode).

.DESCRIPTION
  Cross-platform deployment script for Windows PowerShell / PowerShell 7+.
  Deploys one or all lifecycle environments as isolated compose projects with
  distinct host ports, domain lists, and database identities.

.PARAMETER Environment
  Target environment: development, test, staging, demo, production, or all

.PARAMETER ModuleName
  Target module name (auto-detected from current directory if omitted)

.PARAMETER EnvDir
  Path to env files (default: ./docker/env)

.PARAMETER ComposeDir
  Path to compose files (default: current directory)

.PARAMETER Pull
  docker compose pull before up

.PARAMETER Down
  Stop the selected stack instead of starting

.PARAMETER Validate
  Validate env file before deploy

.PARAMETER ForceRecreate
  Force container recreation

.EXAMPLE
  ../sdkwork-deployments/scripts/deploy.ps1 development
  ../sdkwork-deployments/scripts/deploy.ps1 all -Validate
  ../sdkwork-deployments/scripts/deploy.ps1 production -Down
#>
[CmdletBinding()]
param(
  [Parameter(Position = 0)]
  [ValidateSet('development', 'test', 'staging', 'demo', 'production', 'all')]
  [string]$Environment,

  [string]$ModuleName,

  [string]$EnvDir = './docker/env',

  [string]$ComposeDir = '.',

  [switch]$Pull,

  [switch]$Down,

  [switch]$Validate,

  [switch]$ForceRecreate
)

$ErrorActionPreference = 'Stop'

# ============================================================================
# Functions
# ============================================================================
# NOTE: environment/port/domain/DB mapping is the single source of truth in the
# bash library (scripts/lib/common.sh). This PowerShell port intentionally keeps
# only the helpers it actually uses to avoid drift and dead code.

function Get-ComposeProject {
  param([string]$Environment)
  "${ModuleName}-$Environment"
}

# Resolve the directory containing docker-compose.yml / docker-compose.external.yml.
# Mirrors resolve_compose_dir() in common.sh.
function Resolve-ComposeDir {
  param([string]$UserDir)
  if ($UserDir -and (Test-Path (Join-Path $UserDir 'docker-compose.yml'))) {
    return $UserDir
  }
  if (Test-Path (Join-Path '.' 'docker-compose.yml')) {
    return '.'
  }
  # Bundled templates ship at <repo>/deployments/docker (repo = parent of scripts/).
  $bundled = Join-Path (Split-Path $PSScriptRoot) 'deployments/docker'
  if (Test-Path (Join-Path $bundled 'docker-compose.yml')) {
    return $bundled
  }
  return $(if ($UserDir) { $UserDir } else { '.' })
}

function Deploy-Environment {
  param([string]$EnvName)

  $envFile = Join-Path $EnvDir "$EnvName.env"
  $project = Get-ComposeProject $EnvName

  if (-not (Test-Path $envFile)) {
    Write-Error "missing env file: $envFile`nRun: ../sdkwork-deployments/scripts/generate-env.sh --output-dir $EnvDir"
    exit 1
  }

  # Validate if requested
  if ($Validate -and -not $Down) {
    Write-Host "  -> Validating $EnvName.env" -ForegroundColor Cyan
    $envContent = Get-Content $envFile
    $requiredPatterns = @(
      'HOST_PORT=',
      'POSTGRES_HOST=',
      'POSTGRES_DB=',
      'CLOUDROUTER_REDIS_HOST='
    )
    foreach ($pattern in $requiredPatterns) {
      $found = $envContent | Select-String -Pattern $pattern
      if (-not $found) {
        Write-Error "missing required var pattern: $pattern"
        exit 1
      }
    }
    Write-Host "  [OK] Validation passed" -ForegroundColor Green
  }

  # Build compose arguments
  $composeArgs = @(
    '--env-file', $envFile
    '-p', $project
    '-f', (Join-Path $ComposeDir 'docker-compose.yml')
    '-f', (Join-Path $ComposeDir 'docker-compose.external.yml')
  )

  if ($Down) {
    Write-Host "  -> Stopping $EnvName ($project)" -ForegroundColor Cyan
    & docker compose @composeArgs down
    if ($LASTEXITCODE -ne 0) { throw "docker compose down failed" }
    Write-Host "  [OK] Stopped $EnvName" -ForegroundColor Green
    return
  }

  Write-Host "  -> Deploying $EnvName ($project)" -ForegroundColor Cyan

  if ($Pull) {
    & docker compose @composeArgs pull
    if ($LASTEXITCODE -ne 0) { throw "docker compose pull failed" }
  }

  $upArgs = @('-d')
  if ($ForceRecreate) { $upArgs += '--force-recreate' }

  & docker compose @composeArgs up @upArgs
  if ($LASTEXITCODE -ne 0) { throw "docker compose up failed" }

  # Extract host port
  $portLine = Get-Content $envFile | Select-String -Pattern 'HOST_PORT=' | Select-Object -First 1
  $port = ($portLine -split '=', 2)[1]
  Write-Host "  [OK] Deployed $EnvName -> http://127.0.0.1:${port}/healthz" -ForegroundColor Green
}

# ============================================================================
# Main
# ============================================================================

if ([string]::IsNullOrEmpty($ModuleName)) {
  $ModuleName = (Get-Location).Path.Split([IO.Path]::DirectorySeparatorChar)[-1]
}

Write-Host ""
Write-Host "==> Deploying $ModuleName" -ForegroundColor Yellow

# Check Docker is installed and the daemon is running
if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
  Write-Error "Docker is not installed or not in PATH"
  exit 1
}
try {
  docker info > $null 2>&1
  if ($LASTEXITCODE -ne 0) {
    Write-Error "Docker daemon is not running (start Docker and retry)"
    exit 1
  }
} catch {
  Write-Error "Docker daemon is not running (start Docker and retry)"
  exit 1
}

# Resolve compose file directory (fall back to bundled templates)
$ComposeDir = Resolve-ComposeDir $ComposeDir
Write-Host "  -> Compose dir: $ComposeDir" -ForegroundColor Cyan

switch ($Environment) {
  'all' {
    foreach ($env in @('development', 'test', 'staging', 'demo', 'production')) {
      Deploy-Environment $env
    }
  }
  default {
    Deploy-Environment $Environment
  }
}

Write-Host ""
Write-Host "==> Deployment complete" -ForegroundColor Yellow
