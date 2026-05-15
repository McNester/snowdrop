CREATE TABLE departments (id BIGINT PRIMARY KEY, name VARCHAR(64));
CREATE TABLE worklog (id BIGINT PRIMARY KEY, dept_id BIGINT, hours INT, engineer VARCHAR(64));
