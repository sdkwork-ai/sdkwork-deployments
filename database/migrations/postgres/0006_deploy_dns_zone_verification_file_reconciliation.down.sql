-- Rollback of 0006_deploy_dns_zone_verification_file_reconciliation. The
-- reconciliation only ever ADDS the table the consolidated baseline already
-- declares, so the down migration is a no-op placeholder: dropping it here
-- would destroy data the baseline still requires.
SELECT 1;
