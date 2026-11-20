#!/usr/bin/perl
# A stand-in for OpenRouter on the VM's loopback (T4a, Q2): it speaks the
# `POST …/audio/transcriptions` API the app's real engine uses (a JSON body
# with the WAV in base64; a multipart form is read too), so the signed test
# build (pointed here with --debug-provider-url) goes through its real HTTP
# client, keychain key, retries and JSON parsing, with no network and no
# real key. `POST …/chat/completions` (the cleanup pass) answers with the
# transcript it was sent, unchanged, so cleanup is applied and the paste is
# what the speech reply said; it is logged as `cleanup`, not `request`, so
# the request numbers stay the speech requests'. It is never held.
#
#   perl local-provider.pl <state-dir> [port]      # default port 18731
#
# Plain Perl 5 with core modules only (the VM has no Python). Each connection
# is served in a process of its own, so a held reply doesn't block the next
# request. It is controlled, and read back, through files in <state-dir>:
#
#   accept-key.sha256  the SHA-256 (hex) of the one key it accepts; any other
#                      key, or none, gets a 401 as OpenRouter gives. Only the hash
#                      is on disk, so a test can check that no file holds the
#                      key itself.
#   gate               while this file exists, a reply is held after its
#                      request has been read whole ("the gate"); removing it
#                      releases every held reply.
#   reply-text         the transcript to answer with (UTF-8, as is: leading
#                      and trailing whitespace and newlines are kept);
#                      "hello world" when absent. Read when the request
#                      arrives, so a held reply keeps the text it came with.
#   events             what happened, one ASCII line each, appended:
#                        listening port=<p> pid=<pid>
#                        request n=<n> path=<p> auth=good|bad|none model=<m>
#                          language=<l> format=<f> audio_bytes=<b>
#                        cleanup auth=good|bad|none status=<s>
#                        held n=<n>
#                        closed n=<n>       the client hung up while held
#                        released n=<n> closed=0|1
#                        replied n=<n> status=<s>
#                      <n> counts requests from 1. Never a key or a
#                      transcript.
#   pid                the listener's pid
#
# GET /health answers 200 "ok" and isn't counted.
use strict;
use warnings;
use Digest::SHA qw(sha256_hex);
use Fcntl qw(:flock O_WRONLY O_APPEND O_CREAT);
use IO::Select;
use IO::Socket::INET;
use JSON::PP;

my ($dir, $port) = @ARGV;
die "usage: local-provider.pl <state-dir> [port]\n" unless defined $dir && -d $dir;
$port ||= 18731;
# The longest a reply is held, so a test that forgets the gate can't leave a
# process behind for ever.
my $MAX_HOLD = 120;
my $DEFAULT_TEXT = "hello world";

$SIG{PIPE} = 'IGNORE';
$SIG{CHLD} = 'IGNORE';

my $server = IO::Socket::INET->new(
  LocalAddr => '127.0.0.1',
  LocalPort => $port,
  Listen    => 16,
  ReuseAddr => 1,
  Proto     => 'tcp',
) or die "local-provider: can't listen on 127.0.0.1:$port: $!\n";

write_file("$dir/pid", "$$\n");
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

sub write_file {
  my ($path, $data) = @_;
  open my $fh, '>', "$path.tmp" or die "can't write $path: $!";
  print $fh $data;
  close $fh;
  rename "$path.tmp", $path or die "can't rename $path: $!";
}

# Appends one line to the events file under a lock.
sub event {
  my ($line) = @_;
  sysopen my $fh, "$dir/events", O_WRONLY | O_APPEND | O_CREAT or die "events: $!";
  flock $fh, LOCK_EX;
  syswrite $fh, "$line\n";
  close $fh;
}

# The next request number, taken and logged in one step under the lock.
sub log_request {
  my ($rest) = @_;
  sysopen my $fh, "$dir/events", O_WRONLY | O_APPEND | O_CREAT or die "events: $!";
  flock $fh, LOCK_EX;
  my $n = 1;
  if (open my $in, '<', "$dir/events") {
    while (<$in>) { $n++ if /^request /; }
    close $in;
  }
  syswrite $fh, "request n=$n $rest\n";
  close $fh;
  return $n;
}

# Reads one request: (method, path, lower-cased headers, body), or () if the
# client hung up first.
sub read_request {
  my ($sock) = @_;
  my $data = '';
  my $end;
  while (1) {
    my $got = sysread $sock, my $chunk, 65536;
    return () unless $got;
    $data .= $chunk;
    $end = index $data, "\r\n\r\n";
    last if $end >= 0;
  }
  my $head = substr $data, 0, $end;
  my $body = substr $data, $end + 4;
  my ($request_line, @lines) = split /\r\n/, $head;
  my ($method, $path) = split / /, $request_line;
  my %headers;
  for (@lines) {
    my ($name, $value) = /^([^:]+):\s*(.*)$/ or next;
    $headers{lc $name} = $value;
  }
  if (($headers{'transfer-encoding'} // '') =~ /chunked/i) {
    my $decoded = '';
    while (1) {
      while ($body !~ /\r\n/) {
        my $got = sysread $sock, my $chunk, 65536;
        return () unless $got;
        $body .= $chunk;
      }
      my ($size_hex) = $body =~ /^([0-9a-fA-F]+)/;
      my $size = hex($size_hex // '0');
      my $start = index($body, "\r\n") + 2;
      while (length($body) < $start + $size + 2) {
        my $got = sysread $sock, my $chunk, 65536;
        return () unless $got;
        $body .= $chunk;
      }
      last if $size == 0;
      $decoded .= substr $body, $start, $size;
      $body = substr $body, $start + $size + 2;
    }
    $body = $decoded;
  } else {
    my $length = $headers{'content-length'} // 0;
    while (length($body) < $length) {
      my $got = sysread $sock, my $chunk, 65536;
      return () unless $got;
      $body .= $chunk;
    }
  }
  return ($method, $path, \%headers, $body);
}

# The JSON body as a hash, or undef if it isn't a JSON object.
sub json_body {
  my ($body) = @_;
  my $value = eval { JSON::PP->new->decode($body) };
  return ref $value eq 'HASH' ? $value : undef;
}

# A request field: from the JSON body, else the multipart form; "-" if absent.
sub field {
  my ($json, $body, $name) = @_;
  if ($json) {
    my $value = $json->{$name};
    return defined $value && !ref $value ? $value : '-';
  }
  return form_field($body, $name);
}

# The audio's size in bytes: the base64 `input_audio.data` decoded, or the
# multipart file.
sub audio_bytes {
  my ($json, $body) = @_;
  if ($json) {
    require MIME::Base64;
    my $data = ref $json->{input_audio} eq 'HASH' ? $json->{input_audio}{data} : undef;
    return defined $data ? length MIME::Base64::decode_base64($data) : 0;
  }
  return form_file_bytes($body);
}

# The transcript between the tags of a chat request's user message.
sub cleanup_input {
  my ($json) = @_;
  my $messages = $json && ref $json->{messages} eq 'ARRAY' ? $json->{messages} : [];
  for my $message (@$messages) {
    next unless ref $message eq 'HASH' && ($message->{role} // '') eq 'user';
    my $content = $message->{content} // '';
    return $1 if $content =~ m{<transcript>\n(.*)\n</transcript>}s;
    return $content;
  }
  return '';
}

# A text field of a multipart form, or "-".
sub form_field {
  my ($body, $name) = @_;
  return $1 if $body =~ /name="\Q$name\E"\r\n(?:[^\r\n]+\r\n)*\r\n(.*?)\r\n--/s;
  return '-';
}

sub form_file_bytes {
  my ($body) = @_;
  return length $1 if $body =~ /name="file"[^\r\n]*\r\n(?:[^\r\n]+\r\n)*\r\n(.*?)\r\n--/s;
  return 0;
}

sub respond {
  my ($sock, $status, $reason, $body) = @_;
  my $response = "HTTP/1.1 $status $reason\r\n"
    . "content-type: application/json\r\n"
    . "content-length: " . length($body) . "\r\n"
    . "connection: close\r\n\r\n$body";
  syswrite $sock, $response;
}

# Holds the reply while the gate file exists; says when the client hangs
# up. Returns whether it did.
sub hold {
  my ($sock, $n) = @_;
  return 0 unless -e "$dir/gate";
  event("held n=$n");
  my $select = IO::Select->new($sock);
  my $closed = 0;
  my $until = time + $MAX_HOLD;
  while (-e "$dir/gate" && time < $until) {
    if (!$closed && $select->can_read(0.05)) {
      my $got = sysread $sock, my $chunk, 4096;
      if (!$got) {
        $closed = 1;
        event("closed n=$n");
      }
    } elsif ($closed) {
      select undef, undef, undef, 0.05;
    }
  }
  event("released n=$n closed=$closed");
  return $closed;
}

sub reply_text {
  my $path = "$dir/reply-text";
  return $DEFAULT_TEXT unless -e $path;
  open my $fh, '<:raw', $path or return $DEFAULT_TEXT;
  local $/;
  my $bytes = <$fh>;
  close $fh;
  return Encode::decode('UTF-8', $bytes // '');
}

sub serve {
  my ($sock) = @_;
  require Encode;
  my ($method, $path, $headers, $body) = read_request($sock);
  return unless defined $method;
  if ($method eq 'GET' && $path eq '/health') {
    respond($sock, 200, 'OK', '"ok"');
    return;
  }
  my $auth = 'none';
  if (($headers->{authorization} // '') =~ /^Bearer (\S+)$/) {
    my $want = '';
    if (open my $fh, '<', "$dir/accept-key.sha256") {
      $want = <$fh> // '';
      close $fh;
      $want =~ s/\s+//g;
    }
    $auth = ($want ne '' && sha256_hex($1) eq lc $want) ? 'good' : 'bad';
  }
  my $json = json_body($body);
  if ($method eq 'POST' && $path =~ m{/chat/completions$}) {
    my ($status, $reply) = (401,
      '{"error":{"message":"Invalid API Key","code":401}}');
    if ($auth eq 'good') {
      $reply = JSON::PP->new->ascii->canonical->encode({
        choices => [{ message => { role => 'assistant', content => cleanup_input($json) } }],
      });
      $status = 200;
    }
    respond($sock, $status, $status == 200 ? 'OK' : 'Unauthorized', $reply);
    event("cleanup auth=$auth status=$status");
    return;
  }
  my $fields = sprintf 'model=%s language=%s format=%s audio_bytes=%d',
    field($json, $body, 'model'), field($json, $body, 'language'),
    field($json, $body, 'response_format'), audio_bytes($json, $body);
  $fields =~ s/[^\x20-\x7e]/?/g;
  (my $safe_path = $path) =~ s/[^\x21-\x7e]/?/g;
  my $text = reply_text();
  my $n = log_request("path=$safe_path auth=$auth $fields");
  hold($sock, $n);
  my ($status, $reason, $reply);
  if ($method ne 'POST' || $path !~ m{/audio/transcriptions$}) {
    ($status, $reason, $reply) = (404, 'Not Found', '{"error":{"message":"not found"}}');
  } elsif ($auth ne 'good') {
    ($status, $reason, $reply) = (401, 'Unauthorized',
      '{"error":{"message":"Invalid API Key","type":"invalid_request_error","code":"invalid_api_key"}}');
  } else {
    # ASCII JSON: every non-ASCII character as a \u escape (a non-BMP one as
    # a surrogate pair), which the app's JSON parser must put back together.
    $reply = JSON::PP->new->ascii->canonical->encode({
      text     => $text,
      language => 'english',
      duration => 1.5,
      segments => [{ id => 0, start => 0, end => 1.5, text => $text, no_speech_prob => 0.01 }],
    });
    ($status, $reason) = (200, 'OK');
  }
  respond($sock, $status, $reason, $reply);
  event("replied n=$n status=$status");
}
