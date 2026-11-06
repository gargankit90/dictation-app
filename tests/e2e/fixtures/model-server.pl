#!/usr/bin/perl
# A stand-in for the on-device model's download server, on the VM's loopback
# (U9), so the scenarios that go through a download are deterministic and use no
# internet. The app (pointed here with --debug-model-url) goes through its real
# download: its HTTP client, resume, progress and the catalogue's SHA-256, so the
# file served must be the real model.
#
#   perl model-server.pl <state-dir> <model-file> [port]     # default port 18732
#
# Plain Perl 5 with core modules only (the VM has no Python). Each connection is
# served in a process of its own. Any path but /health serves <model-file>, with
# `Range: bytes=N-` as a 206. Controlled, and read back, through <state-dir>:
#
#   hold-at   while this file exists, from this byte on (its content, a number)
#             only 4 KB a second is sent: slow enough that a download can't
#             finish, and steady enough that the app doesn't call it stalled.
#             Removing it lets the rest go at full speed.
#   events    one line each, appended: listening port=<p> pid=<pid>;
#             request from=<byte>; sent to=<byte>; closed at=<byte>.
#   pid       the listener's pid
use strict;
use warnings;
use Fcntl qw(:flock O_WRONLY O_APPEND O_CREAT);
use IO::Socket::INET;

my ($dir, $file, $port) = @ARGV;
die "usage: model-server.pl <state-dir> <model-file> [port]\n"
  unless defined $dir && defined $file;
$port //= 18732;
my $size = -s $file or die "model-server: no file at $file\n";

$SIG{CHLD} = 'IGNORE';
$SIG{PIPE} = 'IGNORE';

my $server = IO::Socket::INET->new(
  LocalAddr => '127.0.0.1',
  LocalPort => $port,
  Listen    => 16,
  ReuseAddr => 1,
  Proto     => 'tcp',
) or die "model-server: can't listen on 127.0.0.1:$port: $!\n";

open my $pid_file, '>', "$dir/pid" or die "pid: $!";
print $pid_file "$$\n";
close $pid_file;
event("listening port=$port pid=$$");

while (1) {
  my $client = $server->accept or next;
  my $pid = fork;
  if (!defined $pid) {
    close $client;
    next;
  }
  if ($pid == 0) {
    close $server;
    serve($client);
    exit 0;
  }
  close $client;
}

sub event {
  my ($line) = @_;
  sysopen my $fh, "$dir/events", O_WRONLY | O_APPEND | O_CREAT or die "events: $!";
  flock $fh, LOCK_EX;
  syswrite $fh, "$line\n";
  close $fh;
}

# The byte to slow down at, or undef while nothing holds the download.
sub hold_at {
  open my $fh, '<', "$dir/hold-at" or return undef;
  my $at = <$fh>;
  close $fh;
  return defined $at && $at =~ /^(\d+)/ ? $1 : undef;
}

sub serve {
  my ($client) = @_;
  binmode $client;
  my $request = <$client>;
  return unless defined $request;
  my $from = 0;
  while (my $line = <$client>) {
    last if $line =~ /^\r?\n$/;
    $from = $1 if $line =~ /^Range:\s*bytes=(\d+)-/i;
  }
  if ($request =~ m{^GET /health }) {
    print $client "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
    return;
  }
  event("request from=$from");
  if ($from >= $size) {
    print $client "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
    return;
  }
  my $length = $size - $from;
  my $status = $from > 0 ? "206 Partial Content" : "200 OK";
  my $range = $from > 0 ? "Content-Range: bytes $from-" . ($size - 1) . "/$size\r\n" : "";
  print $client "HTTP/1.1 $status\r\nContent-Length: $length\r\n${range}Connection: close\r\n\r\n";
  $client->flush;

  open my $model, '<:raw', $file or die "model: $!";
  seek $model, $from, 0;
  my $at = $from;
  while ($at < $size) {
    my $hold = hold_at();
    my $slow = defined $hold && $at >= $hold;
    my $want = $slow ? 4096 : 1024 * 1024;
    # Not past the hold in one go.
    $want = $hold - $at if defined $hold && !$slow && $hold - $at < $want;
    my $got = read $model, my $chunk, $want;
    last unless $got;
    my $sent = syswrite $client, $chunk;
    if (!defined $sent) {
      event("closed at=$at");
      return;
    }
    # A short write: the next read starts where the client's bytes stopped.
    $at += $sent;
    seek $model, $at, 0 if $sent != $got;
    select undef, undef, undef, 1 if $slow;
  }
  event("sent to=$at");
}
