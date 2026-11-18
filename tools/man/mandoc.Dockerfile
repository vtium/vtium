# mandoc for make man-check on machines that cannot install mandoc.
#
# On Arch, the mandoc package ships its own /usr/bin/man and conflicts with
# man-db, so a desktop that keeps man-db has no mandoc. make man-check then
# builds this image and runs the lint in it. The base is the Ubuntu 24.04
# that CI's runner uses, so the lint here is the same mandoc (1.14.6) that
# CI runs, and a page that passes here passes there.
#
# The base is pinned by digest, so the image changes only when this file
# does. When you change this file, raise the tag that the Makefile's
# MANDOC_IMAGE names, because make man-check reuses an image it finds under
# that tag and never rebuilds it.
FROM ubuntu:24.04@sha256:73b999393d07f2bf65a40373778a44dc9a6d54ad7718d29247d8ff9413c4d6a8

RUN apt-get update \
    && apt-get install -y --no-install-recommends mandoc \
    && rm -rf /var/lib/apt/lists/*

# The lint only reads the pages, so the image runs as an unprivileged user.
# make man-check also passes the invoking user's uid and gid to docker run.
USER nobody
