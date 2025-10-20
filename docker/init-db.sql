-- init-db.sql
-- PostgreSQL initialization script for bloklid database
-- This script runs automatically when the PostgreSQL container is first created

-- Grant permissions on public schema (default)
GRANT ALL PRIVILEGES ON SCHEMA public TO bloklid;
GRANT ALL PRIVILEGES ON ALL TABLES IN SCHEMA public TO bloklid;
GRANT ALL PRIVILEGES ON ALL SEQUENCES IN SCHEMA public TO bloklid;

-- Set default privileges for future tables created by bloklid user
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO bloklid;
ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO bloklid;

-- Enable useful extensions
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";      -- UUID generation functions
CREATE EXTENSION IF NOT EXISTS "pg_stat_statements";  -- Query performance tracking

-- Configure PostgreSQL for optimal performance
ALTER SYSTEM SET shared_buffers = '256MB';
ALTER SYSTEM SET effective_cache_size = '1GB';
ALTER SYSTEM SET maintenance_work_mem = '64MB';
ALTER SYSTEM SET checkpoint_completion_target = 0.9;
ALTER SYSTEM SET wal_buffers = '16MB';
ALTER SYSTEM SET default_statistics_target = 100;
ALTER SYSTEM SET random_page_cost = 1.1;
ALTER SYSTEM SET effective_io_concurrency = 200;
ALTER SYSTEM SET work_mem = '4MB';
ALTER SYSTEM SET min_wal_size = '1GB';
ALTER SYSTEM SET max_wal_size = '4GB';

-- Note: Migrations will create the actual tables
-- This script only sets up permissions and extensions
