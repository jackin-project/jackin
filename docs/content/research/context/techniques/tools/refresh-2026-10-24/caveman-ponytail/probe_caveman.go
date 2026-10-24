package main

import (
 "bytes"
 "encoding/json"
 "fmt"
 "os"
 "path/filepath"
 "github.com/JuliusBrussee/caveman/mcp"
 "time"
 "github.com/JuliusBrussee/caveman/engine"
 "github.com/JuliusBrussee/caveman/engine/ccr"
 "github.com/JuliusBrussee/caveman/engine/tokens"
)

func main() {
 out:=map[string]any{}
 counts:=map[string]any{}
 for _,p:=range os.Args[1:] { b,e:=os.ReadFile(p);if e!=nil {panic(e)};counts[p]=map[string]any{"bytes":len(b),"o200k_base":tokens.Default().Count(b)} }
 out["instruction_counts"]=counts
 data:=[]byte("[\r\n"+string(bytes.Repeat([]byte("{\"state\":\"ok\",\"service\":\"λ雪\",\"value\":7},\r\n"),999))+"{\"state\":\"rare\",\"service\":\"critical\",\"value\":-3}]")
 store,err:=ccr.OpenMemory();if err!=nil {panic(err)};defer store.Close()
 e:=engine.New(store,nil)
 raw,err:=e.Compress(data,engine.Options{});if err!=nil {panic(err)}
 out["default_record_exact"]=bytes.Equal(data,raw.Output)
 noStore,err:=engine.New(nil,nil).Compress(data,engine.Options{Mode:engine.ModeCompress});if err!=nil {panic(err)}
 out["no_store_passes_original"]=bytes.Equal(data,noStore.Output)
 times:=[]float64{}; var r engine.Result
 for i:=0;i<11;i++ { t:=time.Now();r,err=e.Compress(data,engine.Options{Mode:engine.ModeCompress,Query:"critical rare value"});if err!=nil {panic(err)};times=append(times,float64(time.Since(t).Microseconds())/1000) }
 recovered,err:=e.Retrieve(r.RecoveryHandle);if err!=nil {panic(err)}
 out["fixture"]=map[string]any{"bytes":len(data),"content_type":r.ContentType,"tokens_before":r.TokensBefore,"tokens_after":r.TokensAfter,"handle":r.RecoveryHandle,"byte_exact_recovery_crlf_unicode_no_final_newline":bytes.Equal(data,recovered),"compression_ms":times}
 malformed:=[]byte("[{\"a\":1,]")
 r,err=e.Compress(malformed,engine.Options{Mode:engine.ModeCompress,Type:"json"});if err!=nil {panic(err)}
 out["malformed_json_passes_original"]=bytes.Equal(malformed,r.Output)
 _,err=e.Retrieve("ccr_00000000000000000000000000000000");out["missing_recovery_is_error"]=err!=nil
 closed,err:=ccr.OpenMemory();if err!=nil {panic(err)};closed.Close()
 closedResult,closedErr:=engine.New(closed,nil).Compress(data,engine.Options{Mode:engine.ModeCompress})
 out["closed_store_returns_error_and_original"]=closedErr!=nil && bytes.Equal(data,closedResult.Output) && closedResult.RecoveryHandle==""
 dir,err:=os.MkdirTemp("","jackin-ccr-budget-");if err!=nil {panic(err)};defer os.RemoveAll(dir)
 bounded,err:=ccr.OpenWithBudget(filepath.Join(dir,"ccr.db"),1<<20);if err!=nil {panic(err)};defer bounded.Close()
 first:=bytes.Repeat([]byte("a"),600<<10);h,err:=bounded.Put(ccr.Recovery{Original:first,ContentType:"text",Compressor:"fixture"});if err!=nil {panic(err)}
 _,err=bounded.Put(ccr.Recovery{Original:bytes.Repeat([]byte("b"),600<<10),ContentType:"text",Compressor:"fixture"})
 kept,keepErr:=bounded.Get(h)
 out["budget_refuses_new_preserves_old"]=err!=nil && keepErr==nil && bytes.Equal(first,kept)
 toolDefs:=[]map[string]any{}; for _,t:=range mcp.EngineTools(e,nil) {toolDefs=append(toolDefs,map[string]any{"name":t.Name,"description":t.Description,"inputSchema":t.InputSchema,"_meta":t.Meta})}
 defs,err:=json.Marshal(toolDefs);if err!=nil {panic(err)}
 out["mcp_tool_definition_estimate"]=map[string]any{"tools":len(toolDefs),"bytes":len(defs),"o200k_base":tokens.Default().Count(defs),"basis":"local reserialized tool definitions; not billed host wire"}
 b,err:=json.MarshalIndent(out,"","  ");if err!=nil {panic(err)};fmt.Println(string(b))
}
