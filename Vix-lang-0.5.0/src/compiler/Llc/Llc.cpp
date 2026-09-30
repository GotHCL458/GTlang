/*
 * Copyright (c) 2026 Vix Language Authors. All rights reserved.
 * 
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 * 
 *     http://www.apache.org/licenses/LICENSE-2.0
 * 
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */
#include "Llc.h"
#include <llvm/Config/llvm-config.h>
#include <llvm/IR/LegacyPassManager.h>
#include <llvm/IR/LLVMContext.h>
#include <llvm/IR/Module.h>
#include <llvm/IRReader/IRReader.h>
#include <llvm/MC/TargetRegistry.h>
#include <llvm/Support/FileSystem.h>
#include <llvm/Support/MemoryBuffer.h>
#include <llvm/Support/SourceMgr.h>
#include <llvm/Support/TargetSelect.h>
#include <llvm/Support/raw_ostream.h>
#include <llvm/Target/TargetMachine.h>
#include <llvm/Target/TargetOptions.h>
#include <llvm/TargetParser/Host.h>

#include <optional>
#include <mutex>

using namespace llvm;

namespace {

void initializeLLVMTargets() {
	static std::once_flag once;
	std::call_once(once, []() {
		InitializeAllTargetInfos();
		InitializeAllTargets();
		InitializeAllTargetMCs();
		InitializeAllAsmParsers();
		InitializeAllAsmPrinters();
	});
}

std::string formatDiagnostic(const SMDiagnostic &diag) {
	std::string message;
	raw_string_ostream os(message);
	diag.print("llc", os);
	os.flush();
	return message;
}

CodeGenOptLevel toCodeGenOpt(int level) {
	switch (level) {
		case 0:  return CodeGenOptLevel::None;
		case 1:  return CodeGenOptLevel::Less;
		case 2:  return CodeGenOptLevel::Default;
		case 3:  return CodeGenOptLevel::Aggressive;
		default: return CodeGenOptLevel::Default;
	}
}

} // namespace

bool Llc::compileToObject(const std::string &llvm_ir_path,
						  const std::string &out_path,
						  const std::string &triple,
						  bool staticReloc,
						  int optLevel,
						  std::string &errMsg) {
	return compile(llvm_ir_path, out_path, triple, staticReloc, optLevel,
				   CodeGenFileType::ObjectFile, errMsg);
}

bool Llc::compileToAssembly(const std::string &llvm_ir_path,
							const std::string &out_path,
							const std::string &triple,
							bool staticReloc,
							int optLevel,
							std::string &errMsg) {
	return compile(llvm_ir_path, out_path, triple, staticReloc, optLevel,
				   CodeGenFileType::AssemblyFile, errMsg);
}

bool Llc::compile(const std::string &llvm_ir_path,
				  const std::string &out_path,
				  const std::string &triple,
				  bool staticReloc,
				  int optLevel,
				  CodeGenFileType fileType,
				  std::string &errMsg) {
	errMsg.clear();
	initializeLLVMTargets();

	LLVMContext context;
	auto bufferOrErr = MemoryBuffer::getFile(llvm_ir_path);
	if (!bufferOrErr) {
		errMsg = "failed to read LLVM IR file '" + llvm_ir_path + "': " +
				 bufferOrErr.getError().message();
		return false;
	}

	SMDiagnostic diag;
	std::unique_ptr<Module> module = parseIR((*bufferOrErr)->getMemBufferRef(), diag, context);
	if (!module) {
		errMsg = formatDiagnostic(diag);
		return false;
	}

	std::string tripleStr = triple;
	if (tripleStr.empty()) {
#if LLVM_VERSION_MAJOR >= 22
		tripleStr = module->getTargetTriple().str();
#else
		tripleStr = module->getTargetTriple();
#endif
	}
	if (tripleStr.empty()) {
		tripleStr = sys::getDefaultTargetTriple();
	}

	Triple effectiveTriple(tripleStr);
#if LLVM_VERSION_MAJOR >= 22
	module->setTargetTriple(effectiveTriple);
#else
	module->setTargetTriple(effectiveTriple.str());
#endif
	std::string targetError;
	const Target *target = TargetRegistry::lookupTarget(effectiveTriple.str(), targetError);
	if (!target) {
		errMsg = "failed to lookup target for triple '" + tripleStr + "': " + targetError;
		return false;
	}

	TargetOptions options;
	std::string cpu = "generic";
	std::string features;
	auto relocationModel = staticReloc ? std::optional<Reloc::Model>(Reloc::Static)
									   : std::optional<Reloc::Model>(Reloc::PIC_);
	auto codeModel = std::optional<CodeModel::Model>(CodeModel::Small);
	CodeGenOptLevel cgOpt = toCodeGenOpt(optLevel);
	std::unique_ptr<TargetMachine> targetMachine(
#if LLVM_VERSION_MAJOR >= 22
	 target->createTargetMachine(effectiveTriple, cpu, features, options,
	        relocationModel, codeModel, cgOpt));
#else
	 target->createTargetMachine(effectiveTriple.str(), cpu, features, options,
	        relocationModel, codeModel, cgOpt));
#endif
	if (!targetMachine) {
		errMsg = "failed to create target machine for triple '" + tripleStr + "'";
		return false;
	}

	module->setDataLayout(targetMachine->createDataLayout());

	std::error_code fileError;
	raw_fd_ostream output(out_path, fileError, sys::fs::OF_None);
	if (fileError) {
		errMsg = "failed to open output file '" + out_path + "': " + fileError.message();
		return false;
	}

	legacy::PassManager passManager;
	if (targetMachine->addPassesToEmitFile(passManager, output, nullptr, fileType)) {
		errMsg = "target machine cannot emit the requested file type";
		return false;
	}

	passManager.run(*module);
	output.flush();
	return true;
}

extern "C" int llc_compile_to_object(const char *llvm_ir_path,
									  const char *out_path,
									  const char *triple,
									  int staticReloc,
									  int optLevel,
									  const char **errMsg) {
	static thread_local std::string lastError;
	std::string error;
	bool ok = Llc::compileToObject(llvm_ir_path ? llvm_ir_path : "",
								   out_path ? out_path : "",
								   triple ? triple : "",
								   staticReloc != 0,
								   optLevel,
								   error);
	if (!ok) {
		lastError = error;
		if (errMsg) {
			*errMsg = lastError.c_str();
		}
		return 0;
	}

	if (errMsg) {
		*errMsg = nullptr;
	}
	return 1;
}

extern "C" int llc_compile_to_assembly(const char *llvm_ir_path,
										const char *out_path,
										const char *triple,
										int staticReloc,
										int optLevel,
										const char **errMsg) {
	static thread_local std::string lastError;
	std::string error;
	bool ok = Llc::compileToAssembly(llvm_ir_path ? llvm_ir_path : "",
									 out_path ? out_path : "",
									 triple ? triple : "",
									 staticReloc != 0,
									 optLevel,
									 error);
	if (!ok) {
		lastError = error;
		if (errMsg) {
			*errMsg = lastError.c_str();
		}
		return 0;
	}

	if (errMsg) {
		*errMsg = nullptr;
	}
	return 1;
}