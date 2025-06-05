CREATE EXTENSION IF NOT EXISTS postgres_fdw;

CREATE SERVER remote_server;
FOREIGN DATA WRAPPER postgres_fdw
OPTIONS (host '远程IP地址', port '5432', dbname '远程数据库名');

CREATE USER MAPPING FOR 当前用户名
SERVER remote_server
OPTIONS (user '远程用户名', password '远程密码');

IMPORT FOREIGN SCHEMA public
FROM SERVER remote_server
INTO 本地schema名;

ALTER SERVER remote_server OPTIONS (fetch_size '1000000');
ALTER SERVER remote_server OPTIONS (async_capable 'true');