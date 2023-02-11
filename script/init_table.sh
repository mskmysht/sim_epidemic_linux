#!/bin/sh

# psql -U simepi -c "
# create table jobstate (state varchar(20) primary key);
# insert into jobstate (state) values ('Created'), ('Scheduled'), ('Running'), ('Completed');
# "
# psql -U simepi -c "
# create table job (
#     id uuid primary key not null,
#     state varchar(20) references jobstate (state) on update cascade
# );
# "

psql -U simepi -c "
create table taskstate (state varchar(20) primary key);
insert into
    taskstate (state)
    values
        ('Pending'),
        ('Assigned'),
        ('Running'),
        ('Failed'),
        ('Succeeded');
create table task (
    id uuid primary key not null,
    job_id uuid references job (id),
    state varchar(20) references taskstate (state) on delete cascade on update cascade
);
"