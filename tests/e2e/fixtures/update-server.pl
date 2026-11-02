#!/usr/bin/perl
# A stand-in update server on the VM's loopback (M5.4): serves the updater's
# latest.json and the update bundle from a folder, so the test build (which
# trusts only a throwaway key and fetches from http://127.0.0.1:<port>) can
# update itself with no network and never touches the real releases.
#
#   perl update-server.pl <dir> [port]      # default port 18741
#
# Plain Perl 5 with core modules only (the VM has no Python). GET /<name>
# answers the file <dir>/www/<name> (a plain name: letters, digits, ".", "_"
# and "-"; nothing else is ever served), 404 otherwise. Writes:
#
#   <dir>/pid      its own pid, so the test stops exactly this process
#   <dir>/events   "listening port=<p> pid=<pid>", then one line per request:
#                  "get path=<p> status=<s> bytes=<n>"
use strict;
use warnings;
use IO::Socket::INET;

my $dir  = shift @ARGV or die "usage: update-server.pl <dir> [port]\n";
my $port = shift @ARGV || 18741;
my $www  = "$dir/www";

sub event {
    my ($line) = @_;
    open my $fh, '>>', "$dir/events" or return;
    print $fh "$line\n";
    close $fh;
}

my $server = IO::Socket::INET->new(
    LocalAddr => '127.0.0.1',
    LocalPort => $port,
    Proto     => 'tcp',
    Listen    => 16,
    ReuseAddr => 1,
) or die "update-server: can't listen on 127.0.0.1:$port: $!\n";

open my $pidfh, '>', "$dir/pid" or die "update-server: can't write $dir/pid: $!\n";
print $pidfh "$$\n";
close $pidfh;
event("listening port=$port pid=$$");

while (my $client = $server->accept) {
    $client->autoflush(1);
    my $request = <$client> // '';
    # Read the headers to the blank line.
    while (my $header = <$client>) {
        last if $header =~ /^\r?\n$/;
    }
    my ($method, $path) = $request =~ m{^(\S+)\s+(\S+)};
    $method //= '';
    $path   //= '';
    $path =~ s/\?.*//;
    my ($name) = $path =~ m{^/([A-Za-z0-9._-]+)$};
    my $file = defined $name && $name !~ /^\.\.?$/ ? "$www/$name" : undef;
    if (($method eq 'GET' || $method eq 'HEAD') && defined $file && -f $file) {
        open my $fh, '<:raw', $file or next;
        local $/;
        my $body = <$fh> // '';
        close $fh;
        my $type = $name =~ /\.json$/ ? 'application/json' : 'application/octet-stream';
        print $client "HTTP/1.1 200 OK\r\nContent-Type: $type\r\nContent-Length: "
          . length($body) . "\r\nConnection: close\r\n\r\n";
        print $client $body if $method eq 'GET';
        event("get path=$path status=200 bytes=" . length($body));
    } else {
        my $body = "not found\n";
        print $client "HTTP/1.1 404 Not Found\r\nContent-Type: text/plain\r\nContent-Length: "
          . length($body) . "\r\nConnection: close\r\n\r\n$body";
        my $shown = $path =~ m{^[\x21-\x7e]{1,200}$} ? $path : '(unprintable)';
        event("get path=$shown status=404 bytes=0");
    }
    close $client;
}
